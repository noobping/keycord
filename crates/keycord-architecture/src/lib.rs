//! Stable, dependency-free architecture checks for the Keycord repository.
//!
//! This crate intentionally does not depend on Cargo metadata libraries. It can run
//! before the application workspace or its native dependencies are buildable.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const POLICY_DIR: &str = "crates/keycord-architecture/policy";
const WINDOW_NAMESPACE: &str = "keycord-window-fragment";
const SHORTCUTS_NAMESPACE: &str = "keycord-shortcuts-fragment";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Report {
    pub subject_crates: usize,
    pub ui_fragments: usize,
    pub window_actions: usize,
    pub production_rust_files: usize,
    pub legacy_catchall_files: usize,
}

pub fn validate_workspace(root: &Path) -> Result<Report, Vec<String>> {
    let mut checker = Checker::default();
    let subject_manifests = checker.check_workspace_manifests(root);
    let ui_fragments = checker.check_ui_ownership(root);
    let window_actions = checker.check_window_action_ownership(root);
    let legacy_catchall_files = checker.check_root_catchalls(root);
    checker.check_composition_boundaries(root);
    let production_rust_files = checker.check_duplicate_rust(root);

    let report = Report {
        subject_crates: subject_manifests,
        ui_fragments,
        window_actions,
        production_rust_files,
        legacy_catchall_files,
    };

    if checker.violations.is_empty() {
        Ok(report)
    } else {
        checker.violations.sort();
        checker.violations.dedup();
        Err(checker.violations)
    }
}

#[derive(Default)]
struct Checker {
    violations: Vec<String>,
}

impl Checker {
    fn check_workspace_manifests(&mut self, root: &Path) -> usize {
        let root_manifest_path = root.join("Cargo.toml");
        let Some(root_manifest_source) = self.read(&root_manifest_path) else {
            return 0;
        };
        let root_manifest = Manifest::parse(&root_manifest_source);

        let crate_dir = root.join("crates");
        let mut subject_manifests = Vec::new();
        match child_directories(&crate_dir) {
            Ok(directories) => {
                for directory in directories {
                    let manifest = directory.join("Cargo.toml");
                    if manifest.is_file() {
                        subject_manifests.push(manifest);
                    }
                }
            }
            Err(error) => self.io_violation(&crate_dir, error),
        }
        subject_manifests.sort();

        self.check_workspace_members(root, &root_manifest, &subject_manifests);
        self.check_workspace_dependency_defaults(&root_manifest);

        for path in &subject_manifests {
            let Some(source) = self.read(path) else {
                continue;
            };
            let manifest = Manifest::parse(&source);
            let relative = relative_path(root, path);
            let crate_name = manifest
                .value("package", "name")
                .and_then(single_quoted_value)
                .unwrap_or_else(|| {
                    self.violations
                        .push(format!("{relative}: [package].name must be a string"));
                    relative.clone()
                });

            if manifest.value("package", "publish").map(compact) != Some("false".to_string()) {
                self.violations.push(format!(
                    "{relative}: subject crates must set `publish = false`"
                ));
            }
            if manifest.value("features", "default").map(compact) != Some("[]".to_string()) {
                self.violations.push(format!(
                    "{relative}: subject crates must define an empty `default = []` feature"
                ));
            }

            self.check_internal_dependency_defaults(&relative, &manifest);
            self.check_forbidden_edges(&relative, &crate_name, &manifest);
        }

        subject_manifests.len()
    }

    fn check_workspace_members(
        &mut self,
        root: &Path,
        root_manifest: &Manifest,
        subject_manifests: &[PathBuf],
    ) {
        let Some(members) = root_manifest.value("workspace", "members") else {
            self.violations
                .push("Cargo.toml: [workspace].members is required".to_string());
            return;
        };
        let listed = quoted_values(members).into_iter().collect::<BTreeSet<_>>();
        for manifest in subject_manifests {
            let Some(directory) = manifest.parent() else {
                continue;
            };
            let relative = relative_path(root, directory);
            if !listed.contains(&relative) {
                self.violations.push(format!(
                    "Cargo.toml: subject crate `{relative}` is not a workspace member"
                ));
            }
        }
        for member in listed {
            if member.starts_with("crates/keycord-")
                && !root.join(&member).join("Cargo.toml").is_file()
            {
                self.violations.push(format!(
                    "Cargo.toml: workspace member `{member}` has no Cargo.toml"
                ));
            }
        }
    }

    fn check_workspace_dependency_defaults(&mut self, manifest: &Manifest) {
        for assignment in manifest.in_section("workspace.dependencies") {
            if !assignment.key.starts_with("keycord-") {
                continue;
            }
            if !inline_false(&assignment.value, "default-features") {
                self.violations.push(format!(
                    "Cargo.toml: workspace dependency `{}` must set `default-features = false`",
                    assignment.key
                ));
            }
        }
    }

    fn check_internal_dependency_defaults(&mut self, path: &str, manifest: &Manifest) {
        for assignment in manifest.dependency_assignments() {
            let dependency = dependency_name(&assignment.key);
            if !dependency.starts_with("keycord-") {
                continue;
            }
            let value = compact(&assignment.value);
            if value.contains("default-features=true") {
                self.violations.push(format!(
                    "{path}: internal dependency `{dependency}` enables default features"
                ));
            }
            let inherits_workspace = assignment.key.ends_with(".workspace")
                || inline_true(&assignment.value, "workspace");
            let is_direct_path = assignment.value.contains("path");
            if is_direct_path
                && !inherits_workspace
                && !inline_false(&assignment.value, "default-features")
            {
                self.violations.push(format!(
                    "{path}: direct internal dependency `{dependency}` must disable default features"
                ));
            }
        }
    }

    fn check_forbidden_edges(&mut self, path: &str, crate_name: &str, manifest: &Manifest) {
        let all_dependencies = manifest
            .dependency_assignments()
            .map(|assignment| dependency_name(&assignment.key).to_string())
            .collect::<BTreeSet<_>>();

        let opposite = match crate_name {
            "keycord-fido" => Some("keycord-passkey"),
            "keycord-passkey" => Some("keycord-fido"),
            _ => None,
        };
        if let Some(forbidden) = opposite {
            if all_dependencies.contains(forbidden) {
                self.violations.push(format!(
                    "{path}: `{crate_name}` must not depend on `{forbidden}`; FIDO and Passkey are separate subjects"
                ));
            }
        }

        if crate_name == "keycord-lifecycle" {
            for assignment in manifest.normal_dependency_assignments() {
                if dependency_name(&assignment.key) == "keycord-passkey" {
                    self.violations.push(format!(
                        "{path}: Lifecycle must receive Passkey MIME configuration from the composition root, not a normal dependency"
                    ));
                }
            }
        }
    }

    fn check_ui_ownership(&mut self, root: &Path) -> usize {
        let window_skeleton = root.join("crates/keycord-shell/data/window.ui");
        let shortcuts_skeleton = root.join("crates/keycord-shell/data/shortcuts.ui");
        let window_source = self.read(&window_skeleton).unwrap_or_default();
        let shortcuts_source = self.read(&shortcuts_skeleton).unwrap_or_default();

        let mut expected_window = BTreeMap::new();
        let mut expected_shortcuts = BTreeMap::new();
        let mut fragment_count = 0;

        let crates_dir = root.join("crates");
        match recursive_files(&crates_dir) {
            Ok(files) => {
                for path in files {
                    let relative = relative_path(root, &path);
                    if path.extension().and_then(|extension| extension.to_str()) != Some("ui") {
                        continue;
                    }
                    if path == window_skeleton || path == shortcuts_skeleton {
                        continue;
                    }
                    if !relative.ends_with(".fragment.ui") {
                        self.violations.push(format!(
                            "{relative}: declarative subject UI must be a subject-owned `*.fragment.ui` file"
                        ));
                        continue;
                    }
                    fragment_count += 1;
                    let Some((kind, marker)) = fragment_identity(&relative) else {
                        self.violations.push(format!(
                            "{relative}: fragment path must be `crates/keycord-<subject>/data/(window|shortcuts)-<name>.fragment.ui`"
                        ));
                        continue;
                    };
                    let Some(source) = self.read(&path) else {
                        continue;
                    };
                    if source.contains("<interface") {
                        self.violations.push(format!(
                            "{relative}: subject fragments must not declare a standalone `<interface>`"
                        ));
                    }
                    let opposite_namespace = match kind {
                        FragmentKind::Window => SHORTCUTS_NAMESPACE,
                        FragmentKind::Shortcuts => WINDOW_NAMESPACE,
                    };
                    if source.contains(&format!("<!-- {opposite_namespace}:")) {
                        self.violations.push(format!(
                            "{relative}: a {kind:?} fragment must not contain `{opposite_namespace}` markers"
                        ));
                    }
                    let registry = match kind {
                        FragmentKind::Window => &mut expected_window,
                        FragmentKind::Shortcuts => &mut expected_shortcuts,
                    };
                    if let Some(previous) = registry.insert(marker.clone(), relative.clone()) {
                        self.violations.push(format!(
                            "UI fragment marker `{marker}` is owned by both {previous} and {relative}"
                        ));
                    }
                }
            }
            Err(error) => self.io_violation(&crates_dir, error),
        }

        self.check_fragment_registry(
            root,
            FragmentComposition {
                registry_path: "crates/keycord-lifecycle/src/build_support/resources.rs",
                constant: "WINDOW_UI_FRAGMENTS",
                skeleton_path: &window_skeleton,
                skeleton_source: &window_source,
                namespace: WINDOW_NAMESPACE,
                expected: &expected_window,
            },
        );
        self.check_fragment_registry(
            root,
            FragmentComposition {
                registry_path: "crates/keycord-shell/build.rs",
                constant: "SHORTCUTS_FRAGMENTS",
                skeleton_path: &shortcuts_skeleton,
                skeleton_source: &shortcuts_source,
                namespace: SHORTCUTS_NAMESPACE,
                expected: &expected_shortcuts,
            },
        );

        self.check_shell_inventory(root, &window_skeleton, &window_source);
        self.check_shell_inventory(root, &shortcuts_skeleton, &shortcuts_source);
        self.check_no_root_declarative_ui(root);
        fragment_count
    }

    fn check_fragment_registry(&mut self, root: &Path, composition: FragmentComposition<'_>) {
        let FragmentComposition {
            registry_path,
            constant,
            skeleton_path,
            skeleton_source,
            namespace,
            expected,
        } = composition;
        let path = root.join(registry_path);
        let Some(source) = self.read(&path) else {
            return;
        };
        let Some(actual) = fragment_registry(&source, constant) else {
            self.violations.push(format!(
                "{registry_path}: could not read fragment registry `{constant}`"
            ));
            return;
        };
        let actual_map = actual.iter().cloned().collect::<BTreeMap<_, _>>();
        if actual_map.len() != actual.len() {
            self.violations.push(format!(
                "{registry_path}: fragment registry `{constant}` contains duplicate names"
            ));
        }
        for (name, expected_path) in expected {
            match actual_map.get(name) {
                Some(actual_path) if actual_path == expected_path => {}
                Some(actual_path) => self.violations.push(format!(
                    "{registry_path}: fragment `{name}` points to `{actual_path}`, expected `{expected_path}`"
                )),
                None => self.violations.push(format!(
                    "{registry_path}: fragment registry `{constant}` omits `{name}`"
                )),
            }
        }
        for name in actual_map.keys() {
            if !expected.contains_key(name) {
                self.violations.push(format!(
                    "{registry_path}: fragment registry `{constant}` has unexpected entry `{name}`"
                ));
            }
        }

        let mut composed = skeleton_source.to_string();
        for (name, fragment_path) in &actual {
            let marker = format!("<!-- {namespace}:{name} -->");
            let count = composed.matches(&marker).count();
            if count != 1 {
                self.violations.push(format!(
                    "{}: fragment `{name}` must have exactly one reachable marker at its registry position; found {count}",
                    relative_path(root, skeleton_path)
                ));
                continue;
            }
            let marker_start = composed
                .find(&marker)
                .expect("one reachable marker was counted");
            let line_start = composed[..marker_start]
                .rfind('\n')
                .map_or(0, |index| index + 1);
            let line_end = composed[marker_start..]
                .find('\n')
                .map_or(composed.len(), |offset| marker_start + offset);
            if composed[line_start..line_end].trim() != marker {
                self.violations.push(format!(
                    "{}: marker `{name}` must be the only content on its line",
                    relative_path(root, skeleton_path)
                ));
            }
            let fragment_path = root.join(fragment_path);
            let Some(fragment) = self.read(&fragment_path) else {
                continue;
            };
            composed = composed.replacen(&marker, &fragment, 1);
        }
        if composed.contains(&format!("<!-- {namespace}:")) {
            self.violations.push(format!(
                "{}: fragment composition leaves unresolved `{namespace}` markers",
                relative_path(root, skeleton_path)
            ));
        }
    }

    fn check_shell_inventory(&mut self, root: &Path, path: &Path, source: &str) {
        let without_markers = source
            .lines()
            .filter(|line| {
                !line.contains("keycord-window-fragment:")
                    && !line.contains("keycord-shortcuts-fragment:")
            })
            .collect::<Vec<_>>()
            .join("\n");

        let ids = extract_after(&without_markers, "id=\"");
        let mut actions = extract_element_values(&without_markers, "property", "action-name");
        actions.extend(extract_element_values(
            &without_markers,
            "attribute",
            "action",
        ));
        let text = extract_translatable_values(&without_markers);

        self.compare_inventory(root, path, "IDs", "shell-ui-ids.txt", &ids);
        self.compare_inventory(root, path, "actions", "shell-ui-actions.txt", &actions);
        self.compare_inventory(root, path, "translatable text", "shell-ui-text.txt", &text);
    }

    fn check_no_root_declarative_ui(&mut self, root: &Path) {
        // Root data is a generated, flat merge of canonical crate-owned files.
        let directory = root.join("src");
        match recursive_files(&directory) {
            Ok(files) => {
                for path in files {
                    if path.extension().and_then(|extension| extension.to_str()) == Some("ui") {
                        self.violations.push(format!(
                            "{}: declarative UI must live in Shell's skeleton or a subject crate fragment",
                            relative_path(root, &path)
                        ));
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => self.io_violation(&directory, error),
        }
    }

    fn compare_inventory(
        &mut self,
        root: &Path,
        skeleton: &Path,
        description: &str,
        policy_file: &str,
        actual: &BTreeSet<String>,
    ) {
        let policy_path = root.join(POLICY_DIR).join(policy_file);
        let Some(allowed) = self.policy_set(&policy_path) else {
            return;
        };
        for unexpected in actual.difference(&allowed) {
            self.violations.push(format!(
                "{}: unexpected Shell {description} `{unexpected}`; subject UI belongs in its crate fragment",
                relative_path(root, skeleton)
            ));
        }
        // The policies describe both skeletons together, so an allowed item only needs
        // to be present in one of them. Missing entries are checked after combining the
        // inventories in `check_combined_shell_inventory` below.
    }

    fn check_window_action_ownership(&mut self, root: &Path) -> usize {
        let policy_path = root.join(POLICY_DIR).join("window-action-owners.txt");
        let Some(source) = self.read(&policy_path) else {
            return 0;
        };
        let policy = match WindowActionPolicy::parse(&source) {
            Ok(policy) => policy,
            Err(errors) => {
                let relative = relative_path(root, &policy_path);
                self.violations.extend(
                    errors
                        .into_iter()
                        .map(|error| format!("{relative}: {error}")),
                );
                return 0;
            }
        };

        let crates_dir = root.join("crates");
        let subject_directories = match child_directories(&crates_dir) {
            Ok(directories) => directories,
            Err(error) => {
                self.io_violation(&crates_dir, error);
                return policy.rules.len();
            }
        };
        let subjects = subject_directories
            .iter()
            .filter_map(|directory| subject_name(directory))
            .filter(|subject| !is_architecture_support_crate(subject))
            .collect::<BTreeSet<_>>();
        self.check_window_action_policy_subjects(root, &policy_path, &policy, &subjects);

        for directory in subject_directories {
            let Some(subject) = subject_name(&directory) else {
                continue;
            };
            if is_architecture_support_crate(&subject) {
                continue;
            }
            let files = match recursive_files(&directory) {
                Ok(files) => files,
                Err(error) => {
                    self.io_violation(&directory, error);
                    continue;
                }
            };
            for path in files {
                let extension = path.extension().and_then(|extension| extension.to_str());
                if !matches!(extension, Some("rs" | "ui")) || is_test_path(&path) {
                    continue;
                }
                let Some(source) = self.read(&path) else {
                    continue;
                };
                let mentions = match extension {
                    Some("rs") => rust_window_action_mentions(&source, &policy),
                    Some("ui") => ui_window_action_mentions(&source),
                    _ => Vec::new(),
                };
                self.check_subject_action_mentions(root, &path, &subject, &policy, mentions);
            }
        }

        let mut root_rust_files = Vec::new();
        collect_production_rust(
            &root.join("src"),
            &mut root_rust_files,
            &mut self.violations,
        );
        for path in root_rust_files {
            let Some(source) = self.read(&path) else {
                continue;
            };
            for mention in rust_window_action_mentions(&source, &policy)
                .into_iter()
                .filter(|mention| mention.registration)
            {
                if policy.rule_for(&mention.action).is_none() {
                    self.violations.push(format!(
                        "{}:{}: composition registers undeclared window action `{}`; add it to {}",
                        relative_path(root, &path),
                        mention.line,
                        mention.action,
                        relative_path(root, &policy_path),
                    ));
                }
            }
        }

        policy.rules.len()
    }

    fn check_window_action_policy_subjects(
        &mut self,
        root: &Path,
        policy_path: &Path,
        policy: &WindowActionPolicy,
        subjects: &BTreeSet<String>,
    ) {
        let relative = relative_path(root, policy_path);
        for rule in &policy.rules {
            if rule.owner != "root" && !subjects.contains(&rule.owner) {
                self.violations.push(format!(
                    "{relative}:{}: unknown window-action owner `{}`",
                    rule.line, rule.owner
                ));
            }
            for consumer in &rule.consumers {
                if consumer != "all" && !subjects.contains(consumer) {
                    self.violations.push(format!(
                        "{relative}:{}: unknown window-action consumer `{consumer}`",
                        rule.line
                    ));
                }
            }
        }
    }

    fn check_subject_action_mentions(
        &mut self,
        root: &Path,
        path: &Path,
        subject: &str,
        policy: &WindowActionPolicy,
        mentions: Vec<WindowActionMention>,
    ) {
        let relative = relative_path(root, path);
        let mut reported = BTreeSet::new();
        for mention in mentions {
            let Some(rule) = policy.rule_for(&mention.action) else {
                if mention.declaration_required
                    && reported.insert((mention.action.clone(), "undeclared"))
                {
                    self.violations.push(format!(
                        "{relative}:{}: undeclared window action `{}`; add it to policy/window-action-owners.txt",
                        mention.line, mention.action
                    ));
                }
                continue;
            };
            if !rule.allows(subject) && reported.insert((mention.action.clone(), "wrong-owner")) {
                self.violations.push(format!(
                    "{relative}:{}: subject `{subject}` hard-codes window action `{}` owned by `{}`",
                    mention.line, mention.action, rule.owner
                ));
            }
        }
    }

    fn check_root_catchalls(&mut self, root: &Path) -> usize {
        let policy_path = root.join(POLICY_DIR).join("legacy-root-catchalls.txt");
        let Some(allowed) = self.policy_set(&policy_path) else {
            return 0;
        };
        let source_root = root.join("src");
        let mut existing = 0;
        match recursive_files(&source_root) {
            Ok(files) => {
                for path in files {
                    let relative = relative_path(root, &path);
                    if !is_root_catchall(&relative) {
                        if is_forbidden_root_compatibility_path(&relative) {
                            self.violations.push(format!(
                                "{relative}: retired root compatibility paths must not be recreated"
                            ));
                        }
                        continue;
                    }
                    existing += 1;
                    if !allowed.contains(&relative) {
                        self.violations.push(format!(
                            "{relative}: new root catchall files are forbidden; move it to a subject crate or a narrowly named composition module"
                        ));
                    }
                }
            }
            Err(error) => self.io_violation(&source_root, error),
        }
        existing
    }

    fn check_composition_boundaries(&mut self, root: &Path) {
        self.check_no_root_cross_crate_facades(root);
        self.check_window_widget_bundles(root);
        self.check_root_owner_ui_construction(root);
        self.check_fido_ui_ownership(root);
        self.check_fido_service_lifecycle_ownership(root);
        self.check_runtime_capability_ownership(root);
        self.check_installed_branding_asset_ownership(root);
        self.check_stores_key_management_ownership(root);
    }

    fn check_no_root_cross_crate_facades(&mut self, root: &Path) {
        let source_root = root.join("src");
        let mut files = Vec::new();
        collect_production_rust(&source_root, &mut files, &mut self.violations);
        for path in files {
            let Some(source) = self.read(&path) else {
                continue;
            };
            let relative = relative_path(root, &path);
            for line in root_cross_crate_facade_lines(&source) {
                self.violations.push(format!(
                    "{relative}:{line}: root modules must call or import subject APIs directly, not expose `keycord-*` compatibility facades"
                ));
            }
        }
    }

    fn check_window_widget_bundles(&mut self, root: &Path) {
        let policy_path = root.join(POLICY_DIR).join("root-window-widget-bundles.txt");
        let Some(policy) = self.read(&policy_path) else {
            return;
        };
        let expected = named_inventory(&policy);
        let path = root.join("src/window/build/widgets.rs");
        let Some(source) = self.read(&path) else {
            return;
        };
        let relative = relative_path(root, &path);
        let Some(actual) = rust_struct_fields(&source, "WindowWidgets") else {
            self.violations.push(format!(
                "{relative}: could not read the root `WindowWidgets` composition registry"
            ));
            return;
        };

        for (field, expected_type) in &expected {
            match actual.get(field) {
                Some(actual_type) if actual_type == expected_type => {}
                Some(actual_type) => self.violations.push(format!(
                    "{relative}: root widget bundle `{field}` has type `{actual_type}`, expected `{expected_type}`"
                )),
                None => self.violations.push(format!(
                    "{relative}: root widget composition omits bundle `{field}: {expected_type}`"
                )),
            }
        }
        for (field, actual_type) in &actual {
            if !expected.contains_key(field) {
                self.violations.push(format!(
                    "{relative}: unexpected flat root widget field `{field}: {actual_type}`; add widgets to their subject-owned bundle"
                ));
            }
        }
    }

    fn check_root_owner_ui_construction(&mut self, root: &Path) {
        let policy_path = root
            .join(POLICY_DIR)
            .join("root-forbidden-owner-ui-construction.txt");
        let Some(forbidden) = self.policy_set(&policy_path) else {
            return;
        };
        let source_root = root.join("src");
        let mut files = Vec::new();
        collect_production_rust(&source_root, &mut files, &mut self.violations);
        for path in files {
            let Some(source) = self.read(&path) else {
                continue;
            };
            let relative = relative_path(root, &path);
            let masked = production_rust_code_mask(&source);
            for marker in &forbidden {
                for (offset, _) in masked
                    .match_indices(marker)
                    .filter(|(offset, _)| owner_ui_marker_is_construction(&masked, *offset, marker))
                {
                    self.violations.push(format!(
                        "{relative}:{}: root reconstructs owner UI with `{marker}`; pass generic chrome/ports to the subject-owned constructor instead",
                        line_number_at(&masked, offset)
                    ));
                }
            }
        }
    }

    fn check_fido_ui_ownership(&mut self, root: &Path) {
        self.check_fido_root_widget_composition(root);

        let api_policy_path = root.join(POLICY_DIR).join("keys-fido-ui-api.txt");
        let forbidden_keys_policy_path = root
            .join(POLICY_DIR)
            .join("keys-forbidden-fido-ui-ownership.txt");
        let forbidden_root_policy_path = root
            .join(POLICY_DIR)
            .join("root-forbidden-fido-presentation.txt");
        let Some(expected_api) = self.policy_set(&api_policy_path) else {
            return;
        };
        let Some(forbidden_keys_markers) = self.policy_set(&forbidden_keys_policy_path) else {
            return;
        };
        let Some(forbidden_root_markers) = self.policy_set(&forbidden_root_policy_path) else {
            return;
        };

        let keys_ui = root.join("crates/keycord-keys/src/ui");
        let mut actual_api = BTreeSet::new();
        let mut files = Vec::new();
        collect_production_rust(&keys_ui, &mut files, &mut self.violations);
        for path in files {
            let Some(source) = self.read(&path) else {
                continue;
            };
            let relative = relative_path(root, &path);
            let (items, invalid_references) = subject_ui_api_items(&source, "keycord_fido");
            actual_api.extend(items);
            for (line, reference) in invalid_references {
                self.violations.push(format!(
                    "{relative}:{line}: Keys UI must consume reviewed `keycord_fido::ui` APIs, not `{reference}`"
                ));
            }
            self.check_forbidden_production_markers(
                &relative,
                &source,
                &forbidden_keys_markers,
                "FIDO generation presentation belongs to FIDO; Keys may retain only its OpenPGP adapter",
            );
        }
        compare_policy_inventory(
            &mut self.violations,
            "crates/keycord-keys/src/ui",
            "reviewed FIDO UI API",
            &actual_api,
            &expected_api,
            "Keys may consume only the explicit FIDO-owned presentation contract",
        );

        let root_source = root.join("src");
        let mut root_files = Vec::new();
        collect_production_rust(&root_source, &mut root_files, &mut self.violations);
        for path in root_files {
            let Some(source) = self.read(&path) else {
                continue;
            };
            self.check_forbidden_production_markers(
                &relative_path(root, &path),
                &source,
                &forbidden_root_markers,
                "FIDO presentation belongs to the FIDO subject",
            );
        }
    }

    fn check_fido_root_widget_composition(&mut self, root: &Path) {
        let path = root.join("src/window/build/widgets.rs");
        let Some(source) = self.read(&path) else {
            return;
        };
        let relative = relative_path(root, &path);
        for marker in [
            "use keycord_fido::ui::FidoWindowWidgets;",
            "pub(in crate::window) fido: FidoWindowWidgets,",
            "fido: FidoWindowWidgets::load(builder)?,",
        ] {
            if !item_has_immediate_cfg_feature(&source, marker, "fidokey") {
                self.violations.push(format!(
                    "{relative}: `{marker}` must exist immediately below `#[cfg(feature = \"fidokey\")]`; FIDO is a conditional owner bundle"
                ));
            }
        }
    }

    fn check_fido_service_lifecycle_ownership(&mut self, root: &Path) {
        let policy_path = root
            .join(POLICY_DIR)
            .join("keys-forbidden-fido-service-lifecycle.txt");
        let Some(forbidden) = self.policy_set(&policy_path) else {
            return;
        };
        let adapter_root = root.join("crates/keycord-keys/src/fido2");
        let files = match recursive_files(&adapter_root) {
            Ok(files) => files,
            Err(error) => {
                self.io_violation(&adapter_root, error);
                return;
            }
        };
        for path in files
            .into_iter()
            .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("rs"))
        {
            let Some(source) = self.read(&path) else {
                continue;
            };
            let relative = relative_path(root, &path);
            for marker in &forbidden {
                for (offset, _) in source.match_indices(marker) {
                    self.violations.push(format!(
                        "{relative}:{}: FIDO service lifecycle marker `{marker}` belongs to FIDO; Keys must use the shared FIDO service adapter",
                        line_number_at(&source, offset)
                    ));
                }
            }
        }
    }

    fn check_forbidden_production_markers(
        &mut self,
        relative: &str,
        source: &str,
        forbidden: &BTreeSet<String>,
        ownership_hint: &str,
    ) {
        let masked = production_rust_code_mask(source);
        let literals = production_rust_string_literals(source);
        for marker in forbidden {
            for (offset, _) in masked.match_indices(marker) {
                self.violations.push(format!(
                    "{relative}:{}: forbidden owner marker `{marker}`; {ownership_hint}",
                    line_number_at(&masked, offset)
                ));
            }
            for literal in literals
                .iter()
                .filter(|literal| literal.value.contains(marker))
            {
                self.violations.push(format!(
                    "{relative}:{}: forbidden owner text `{marker}`; {ownership_hint}",
                    line_number_at(source, literal.start)
                ));
            }
        }
    }

    fn check_runtime_capability_ownership(&mut self, root: &Path) {
        let feature_policy_path = root.join(POLICY_DIR).join("runtime-features.txt");
        let capability_policy_path = root
            .join(POLICY_DIR)
            .join("runtime-capability-functions.txt");
        let toml_policy_path = root
            .join(POLICY_DIR)
            .join("runtime-bounded-toml-policy-items.txt");
        let Some(expected_features) = self.policy_set(&feature_policy_path) else {
            return;
        };
        let Some(expected_capabilities) = self.policy_set(&capability_policy_path) else {
            return;
        };
        let Some(expected_toml_policy_items) = self.policy_set(&toml_policy_path) else {
            return;
        };

        let manifest_path = root.join("crates/keycord-runtime/Cargo.toml");
        let Some(manifest_source) = self.read(&manifest_path) else {
            return;
        };
        let manifest = Manifest::parse(&manifest_source);
        let actual_features = manifest
            .in_section("features")
            .map(|assignment| assignment.key.clone())
            .collect::<BTreeSet<_>>();
        compare_policy_inventory(
            &mut self.violations,
            "crates/keycord-runtime/Cargo.toml",
            "Runtime feature",
            &actual_features,
            &expected_features,
            "subject features belong to their owner crates",
        );

        let capabilities_path = root.join("crates/keycord-runtime/src/capabilities.rs");
        let Some(capabilities_source) = self.read(&capabilities_path) else {
            return;
        };
        let actual_capabilities = public_rust_function_names(&capabilities_source);
        compare_policy_inventory(
            &mut self.violations,
            "crates/keycord-runtime/src/capabilities.rs",
            "Runtime capability function",
            &actual_capabilities,
            &expected_capabilities,
            "subject capability probes belong to their owner crates",
        );

        let bounded_toml_path = root.join("crates/keycord-runtime/src/bounded_toml.rs");
        let Some(bounded_toml_source) = self.read(&bounded_toml_path) else {
            return;
        };
        let actual_toml_policy_items = public_top_level_policy_items(&bounded_toml_source);
        compare_policy_inventory(
            &mut self.violations,
            "crates/keycord-runtime/src/bounded_toml.rs",
            "Runtime bounded-TOML policy item",
            &actual_toml_policy_items,
            &expected_toml_policy_items,
            "subject-specific limits and policy types belong to their owner crates",
        );

        self.check_runtime_subject_identifiers(root);
    }

    fn check_runtime_subject_identifiers(&mut self, root: &Path) {
        let policy_path = root
            .join(POLICY_DIR)
            .join("runtime-forbidden-subject-identifiers.txt");
        let Some(forbidden) = self.policy_set(&policy_path) else {
            return;
        };
        let runtime_source = root.join("crates/keycord-runtime/src");
        let mut files = Vec::new();
        collect_production_rust(&runtime_source, &mut files, &mut self.violations);
        for path in files {
            let Some(source) = self.read(&path) else {
                continue;
            };
            let relative = relative_path(root, &path);
            for identifier in &forbidden {
                for (offset, _) in source.match_indices(identifier) {
                    self.violations.push(format!(
                        "{relative}:{}: Runtime contains subject identifier `{identifier}`; move its policy to the owning crate",
                        line_number_at(&source, offset)
                    ));
                }
            }
        }
    }

    fn check_installed_branding_asset_ownership(&mut self, root: &Path) {
        let shell_data = root.join("crates/keycord-shell/data");
        match recursive_files(&shell_data) {
            Ok(files) => {
                for path in files {
                    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                        continue;
                    };
                    if name.starts_with("io.github.noobping.keycord") {
                        self.violations.push(format!(
                            "{}: packaged application branding assets belong to Lifecycle, not Shell's in-app chrome",
                            relative_path(root, &path)
                        ));
                    }
                }
            }
            Err(error) => self.io_violation(&shell_data, error),
        }
    }

    fn check_stores_key_management_ownership(&mut self, root: &Path) {
        let recipient_ui = root.join("crates/keycord-stores/src/ui/recipient_page");
        let retired_export = recipient_ui.join("export.rs");
        if retired_export.exists() {
            self.violations.push(format!(
                "{}: key-material export belongs to Keys; this retired Stores implementation must not be recreated",
                relative_path(root, &retired_export)
            ));
        }

        let api_policy_path = root
            .join(POLICY_DIR)
            .join("stores-recipient-keys-ui-api.txt");
        let forbidden_policy_path = root
            .join(POLICY_DIR)
            .join("stores-forbidden-key-management-identifiers.txt");
        let Some(expected_api) = self.policy_set(&api_policy_path) else {
            return;
        };
        let Some(forbidden_identifiers) = self.policy_set(&forbidden_policy_path) else {
            return;
        };

        let mut actual_api = BTreeSet::new();
        let mut files = Vec::new();
        collect_production_rust(&recipient_ui, &mut files, &mut self.violations);
        for path in files {
            let Some(source) = self.read(&path) else {
                continue;
            };
            let relative = relative_path(root, &path);
            let (items, invalid_references) = stores_keys_ui_api_items(&source);
            actual_api.extend(items);
            for (line, reference) in invalid_references {
                self.violations.push(format!(
                    "{relative}:{line}: Stores recipient UI must consume reviewed `keycord_keys::ui` controller APIs, not `{reference}`"
                ));
            }

            let masked = production_rust_code_mask(&source);
            for identifier in &forbidden_identifiers {
                for (offset, _) in masked.match_indices(identifier) {
                    self.violations.push(format!(
                        "{relative}:{}: direct key-management identifier `{identifier}` belongs to Keys; Stores may retain recipient selection policy only",
                        line_number_at(&masked, offset)
                    ));
                }
            }
        }
        compare_policy_inventory(
            &mut self.violations,
            "crates/keycord-stores/src/ui/recipient_page",
            "reviewed Keys UI controller API",
            &actual_api,
            &expected_api,
            "Stores may consume only the explicit Keys-owned recipient controller contract",
        );

        self.check_recipient_ui_id_ownership(root);
    }

    fn check_recipient_ui_id_ownership(&mut self, root: &Path) {
        let policy_path = root.join(POLICY_DIR).join("recipient-ui-id-owners.txt");
        let Some(policy) = self.read(&policy_path) else {
            return;
        };
        let expected = named_inventory(&policy);
        let mut actual: BTreeMap<String, (String, String)> = BTreeMap::new();

        for owner in ["fido", "keys", "stores"] {
            let data_dir = root.join(format!("crates/keycord-{owner}/data"));
            let files = match recursive_files(&data_dir) {
                Ok(files) => files,
                Err(error) => {
                    self.io_violation(&data_dir, error);
                    continue;
                }
            };
            for path in files.into_iter().filter(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("ui")
            }) {
                let Some(source) = self.read(&path) else {
                    continue;
                };
                let relative = relative_path(root, &path);
                for id in extract_after(&source, "id=\"")
                    .into_iter()
                    .filter(|id| id.starts_with("store_recipients_"))
                {
                    if let Some((previous_owner, previous_path)) =
                        actual.insert(id.clone(), (owner.to_string(), relative.clone()))
                    {
                        self.violations.push(format!(
                            "{relative}: recipient UI id `{id}` duplicates {previous_owner}-owned `{previous_path}`"
                        ));
                    }
                }
            }
        }

        for (id, expected_owner) in &expected {
            match actual.get(id) {
                Some((actual_owner, _)) if actual_owner == expected_owner => {}
                Some((actual_owner, path)) => self.violations.push(format!(
                    "{path}: recipient UI id `{id}` belongs to {expected_owner}, not {actual_owner}"
                )),
                None => self.violations.push(format!(
                    "{}: expected {expected_owner}-owned recipient UI id `{id}` is missing",
                    relative_path(root, &policy_path)
                )),
            }
        }
        for (id, (owner, path)) in &actual {
            if !expected.contains_key(id) {
                self.violations.push(format!(
                    "{path}: unexpected {owner}-owned recipient UI id `{id}`; declare its reviewed owner in policy/recipient-ui-id-owners.txt"
                ));
            }
        }
    }

    fn check_duplicate_rust(&mut self, root: &Path) -> usize {
        let mut files = Vec::new();
        let root_src = root.join("src");
        collect_production_rust(&root_src, &mut files, &mut self.violations);

        let crates_dir = root.join("crates");
        if let Ok(crates) = child_directories(&crates_dir) {
            for crate_dir in crates {
                collect_production_rust(&crate_dir.join("src"), &mut files, &mut self.violations);
            }
        }
        files.sort();

        let mut exact_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut exact_functions: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for path in &files {
            let Some(source) = self.read(path) else {
                continue;
            };
            let relative = relative_path(root, path);
            let normalized = source.replace("\r\n", "\n");
            if substantial(&normalized, 12, 240) {
                exact_files
                    .entry(normalized.trim().to_string())
                    .or_default()
                    .push(relative.clone());
            }
            for function in rust_functions(&normalized) {
                if substantial(&function.body, 9, 180)
                    && !is_trivial_struct_literal_adapter(&function.body)
                {
                    exact_functions
                        .entry(function.body.trim().to_string())
                        .or_default()
                        .push(format!("{relative}::{}", function.name));
                }
            }
        }

        for paths in exact_files.values().filter(|paths| paths.len() > 1) {
            self.violations.push(format!(
                "exact duplicate production Rust files: {}",
                paths.join(", ")
            ));
        }
        for functions in exact_functions
            .values()
            .filter(|functions| functions.len() > 1)
        {
            self.violations.push(format!(
                "exact duplicate production Rust function bodies: {}",
                functions.join(", ")
            ));
        }

        files.len()
    }

    fn policy_set(&mut self, path: &Path) -> Option<BTreeSet<String>> {
        self.read(path).map(|source| policy_lines(&source))
    }

    fn read(&mut self, path: &Path) -> Option<String> {
        match fs::read_to_string(path) {
            Ok(source) => Some(source),
            Err(error) => {
                self.io_violation(path, error);
                None
            }
        }
    }

    fn io_violation(&mut self, path: &Path, error: io::Error) {
        self.violations.push(format!("{}: {error}", path.display()));
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WindowActionRule {
    owner: String,
    pattern: String,
    consumers: BTreeSet<String>,
    line: usize,
}

impl WindowActionRule {
    fn matches(&self, action: &str) -> bool {
        self.pattern
            .strip_suffix('*')
            .map_or(action == self.pattern, |prefix| action.starts_with(prefix))
    }

    fn allows(&self, subject: &str) -> bool {
        self.owner == subject || self.consumers.contains("all") || self.consumers.contains(subject)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct WindowActionPolicy {
    rules: Vec<WindowActionRule>,
}

impl WindowActionPolicy {
    fn parse(source: &str) -> Result<Self, Vec<String>> {
        let mut rules = Vec::new();
        let mut errors = Vec::new();
        for (index, raw_line) in source.lines().enumerate() {
            let line_number = index + 1;
            let line = raw_line.split('#').next().unwrap_or_default().trim();
            if line.is_empty() {
                continue;
            }
            let fields = line.split_whitespace().collect::<Vec<_>>();
            if !(2..=3).contains(&fields.len()) {
                errors.push(format!(
                    "line {line_number}: expected `OWNER ACTION [ALLOWED-CONSUMERS]`"
                ));
                continue;
            }
            let owner = fields[0];
            let pattern = fields[1];
            if owner != "root" && !valid_subject_name(owner) {
                errors.push(format!(
                    "line {line_number}: invalid window-action owner `{owner}`"
                ));
                continue;
            }
            if !valid_action_pattern(pattern) {
                errors.push(format!(
                    "line {line_number}: invalid window-action pattern `{pattern}`"
                ));
                continue;
            }
            let consumers = fields.get(2).map_or_else(BTreeSet::new, |value| {
                value
                    .split(',')
                    .map(str::to_string)
                    .collect::<BTreeSet<_>>()
            });
            if consumers.contains("")
                || (consumers.contains("all") && consumers.len() != 1)
                || consumers
                    .iter()
                    .any(|consumer| consumer != "all" && !valid_subject_name(consumer))
            {
                errors.push(format!(
                    "line {line_number}: invalid allowed-consumer list `{}`",
                    fields.get(2).copied().unwrap_or_default()
                ));
                continue;
            }
            rules.push(WindowActionRule {
                owner: owner.to_string(),
                pattern: pattern.to_string(),
                consumers,
                line: line_number,
            });
        }

        for (left_index, left) in rules.iter().enumerate() {
            for right in rules.iter().skip(left_index + 1) {
                if action_patterns_overlap(&left.pattern, &right.pattern) {
                    errors.push(format!(
                        "lines {} and {}: overlapping window-action patterns `{}` and `{}`",
                        left.line, right.line, left.pattern, right.pattern
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(Self { rules })
        } else {
            Err(errors)
        }
    }

    fn rule_for(&self, action: &str) -> Option<&WindowActionRule> {
        self.rules.iter().find(|rule| rule.matches(action))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WindowActionMention {
    action: String,
    line: usize,
    declaration_required: bool,
    registration: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RustStringLiteral {
    value: String,
    start: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Assignment {
    section: String,
    key: String,
    value: String,
}

struct FragmentComposition<'a> {
    registry_path: &'a str,
    constant: &'a str,
    skeleton_path: &'a Path,
    skeleton_source: &'a str,
    namespace: &'a str,
    expected: &'a BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct Manifest {
    assignments: Vec<Assignment>,
}

impl Manifest {
    fn parse(source: &str) -> Self {
        let mut assignments = Vec::new();
        let mut section = String::new();
        let mut pending: Option<(String, String, String)> = None;

        for raw_line in source.lines() {
            let line = strip_toml_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if let Some((pending_section, key, mut value)) = pending.take() {
                value.push(' ');
                value.push_str(line);
                if delimiters_balanced(&value) {
                    assignments.push(Assignment {
                        section: pending_section,
                        key,
                        value,
                    });
                } else {
                    pending = Some((pending_section, key, value));
                }
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string();
                continue;
            }
            let Some(equals) = unquoted_index(line, '=') else {
                continue;
            };
            let key = line[..equals].trim().to_string();
            let value = line[equals + 1..].trim().to_string();
            if delimiters_balanced(&value) {
                assignments.push(Assignment {
                    section: section.clone(),
                    key,
                    value,
                });
            } else {
                pending = Some((section.clone(), key, value));
            }
        }

        Self { assignments }
    }

    fn value(&self, section: &str, key: &str) -> Option<&str> {
        self.assignments
            .iter()
            .find(|assignment| assignment.section == section && assignment.key == key)
            .map(|assignment| assignment.value.as_str())
    }

    fn in_section<'a>(&'a self, section: &'a str) -> impl Iterator<Item = &'a Assignment> {
        self.assignments
            .iter()
            .filter(move |assignment| assignment.section == section)
    }

    fn dependency_assignments(&self) -> impl Iterator<Item = &Assignment> {
        self.assignments
            .iter()
            .filter(|assignment| is_dependency_section(&assignment.section))
    }

    fn normal_dependency_assignments(&self) -> impl Iterator<Item = &Assignment> {
        self.assignments.iter().filter(|assignment| {
            is_dependency_section(&assignment.section)
                && !assignment.section.contains("build-dependencies")
                && !assignment.section.contains("dev-dependencies")
        })
    }
}

fn is_dependency_section(section: &str) -> bool {
    section == "dependencies"
        || section == "build-dependencies"
        || section == "dev-dependencies"
        || section.ends_with(".dependencies")
        || section.ends_with(".build-dependencies")
        || section.ends_with(".dev-dependencies")
}

fn dependency_name(key: &str) -> &str {
    key.strip_suffix(".workspace").unwrap_or(key)
}

fn strip_toml_comment(line: &str) -> &str {
    let mut escaped = false;
    let mut quote = None;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote == Some('"') {
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
            continue;
        }
        if character == '#' && quote.is_none() {
            return &line[..index];
        }
    }
    line
}

fn unquoted_index(value: &str, needle: char) -> Option<usize> {
    let mut escaped = false;
    let mut quote = None;
    for (index, character) in value.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' && quote == Some('"') {
            escaped = true;
        } else if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
        } else if character == needle && quote.is_none() {
            return Some(index);
        }
    }
    None
}

fn delimiters_balanced(value: &str) -> bool {
    let mut escaped = false;
    let mut quote = None;
    let mut braces = 0_i32;
    let mut brackets = 0_i32;
    for character in value.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' && quote == Some('"') {
            escaped = true;
            continue;
        }
        if matches!(character, '\'' | '"') {
            if quote == Some(character) {
                quote = None;
            } else if quote.is_none() {
                quote = Some(character);
            }
            continue;
        }
        if quote.is_some() {
            continue;
        }
        match character {
            '{' => braces += 1,
            '}' => braces -= 1,
            '[' => brackets += 1,
            ']' => brackets -= 1,
            _ => {}
        }
    }
    quote.is_none() && braces == 0 && brackets == 0
}

fn compact(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn inline_false(value: &str, key: &str) -> bool {
    compact(value).contains(&format!("{key}=false"))
}

fn inline_true(value: &str, key: &str) -> bool {
    compact(value).contains(&format!("{key}=true"))
}

fn single_quoted_value(value: &str) -> Option<String> {
    let values = quoted_values(value);
    (values.len() == 1).then(|| values[0].clone())
}

fn quoted_values(value: &str) -> Vec<String> {
    let mut values = Vec::new();
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if !matches!(bytes[index], b'"' | b'\'') {
            index += 1;
            continue;
        }
        let quote = bytes[index];
        index += 1;
        let start = index;
        let mut escaped = false;
        while index < bytes.len() {
            if escaped {
                escaped = false;
            } else if bytes[index] == b'\\' && quote == b'"' {
                escaped = true;
            } else if bytes[index] == quote {
                values.push(value[start..index].to_string());
                index += 1;
                break;
            }
            index += 1;
        }
    }
    values
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FragmentKind {
    Window,
    Shortcuts,
}

fn fragment_identity(relative: &str) -> Option<(FragmentKind, String)> {
    let components = relative.split('/').collect::<Vec<_>>();
    if components.len() != 4 || components[0] != "crates" || components[2] != "data" {
        return None;
    }
    let subject = components[1].strip_prefix("keycord-")?;
    if subject == "shell" || subject == "ui-fragments" {
        return None;
    }
    let filename = components[3].strip_suffix(".fragment.ui")?;
    let (kind, rest) = if let Some(rest) = filename.strip_prefix("window-") {
        (FragmentKind::Window, rest)
    } else {
        let rest = filename.strip_prefix("shortcuts-")?;
        (FragmentKind::Shortcuts, rest)
    };
    if rest.is_empty() {
        return None;
    }
    Some((kind, format!("{subject}-{rest}")))
}

fn fragment_registry(source: &str, constant: &str) -> Option<Vec<(String, String)>> {
    let declaration = format!("const {constant}:");
    let start = source.find(&declaration)?;
    let array_start = source[start..]
        .find("&[")
        .map(|offset| start + offset + 2)?;
    let array_end = source[array_start..]
        .find("];")
        .map(|offset| array_start + offset)?;
    let values = quoted_values(&source[array_start..array_end]);
    if !values.chunks_exact(2).remainder().is_empty() {
        return None;
    }
    let mut registry = Vec::new();
    for pair in values.chunks_exact(2) {
        registry.push((pair[0].clone(), pair[1].clone()));
    }
    Some(registry)
}

fn extract_after(source: &str, prefix: &str) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    let mut remaining = source;
    while let Some(start) = remaining.find(prefix) {
        remaining = &remaining[start + prefix.len()..];
        let Some(end) = remaining.find('"') else {
            break;
        };
        values.insert(remaining[..end].to_string());
        remaining = &remaining[end + 1..];
    }
    values
}

fn extract_element_values(source: &str, tag: &str, name: &str) -> BTreeSet<String> {
    let opening = format!("<{tag} name=\"{name}\">");
    let mut values = BTreeSet::new();
    let mut remaining = source;
    while let Some(start) = remaining.find(&opening) {
        remaining = &remaining[start + opening.len()..];
        let Some(end) = remaining.find('<') else {
            break;
        };
        values.insert(remaining[..end].trim().to_string());
        remaining = &remaining[end..];
    }
    values
}

fn extract_translatable_values(source: &str) -> BTreeSet<String> {
    let mut values = BTreeSet::new();
    let mut remaining = source;
    while let Some(attribute) = remaining.find("translatable=\"yes\"") {
        remaining = &remaining[attribute..];
        let Some(tag_end) = remaining.find('>') else {
            break;
        };
        remaining = &remaining[tag_end + 1..];
        let Some(text_end) = remaining.find('<') else {
            break;
        };
        let value = remaining[..text_end].trim();
        if !value.is_empty() {
            values.insert(value.to_string());
        }
        remaining = &remaining[text_end..];
    }
    values
}

fn policy_lines(source: &str) -> BTreeSet<String> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn named_inventory(source: &str) -> BTreeMap<String, String> {
    policy_lines(source)
        .into_iter()
        .filter_map(|line| {
            let (name, value) = line.split_once(char::is_whitespace)?;
            Some((name.to_string(), value.trim().to_string()))
        })
        .collect()
}

fn compare_policy_inventory(
    violations: &mut Vec<String>,
    path: &str,
    description: &str,
    actual: &BTreeSet<String>,
    expected: &BTreeSet<String>,
    ownership_hint: &str,
) {
    for unexpected in actual.difference(expected) {
        violations.push(format!(
            "{path}: unexpected {description} `{unexpected}`; {ownership_hint}"
        ));
    }
    for missing in expected.difference(actual) {
        violations.push(format!(
            "{path}: expected {description} `{missing}` is missing; update its explicit policy if the generic contract intentionally changed"
        ));
    }
}

fn public_item_remainder(line: &str) -> Option<&str> {
    let line = line.trim_start();
    if let Some(rest) = line.strip_prefix("pub ") {
        return Some(rest.trim_start());
    }
    let rest = line.strip_prefix("pub(")?;
    let close = rest.find(')')?;
    Some(rest[close + 1..].trim_start())
}

fn is_root_cross_crate_facade(line: &str) -> bool {
    let Some(item) = public_item_remainder(line) else {
        return false;
    };
    let item = item.split_whitespace().collect::<Vec<_>>().join(" ");
    item.strip_prefix("use ")
        .is_some_and(|target| target.trim_start().starts_with("keycord_"))
        || (item.starts_with("type ") && item.contains("= keycord_"))
}

fn root_cross_crate_facade_lines(source: &str) -> Vec<usize> {
    let masked = rust_code_mask(source);
    let mut facades = Vec::new();
    let mut pending: Option<(usize, String)> = None;
    for (index, line) in masked.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if let Some((start, statement)) = pending.as_mut() {
            statement.push(' ');
            statement.push_str(trimmed);
            if trimmed.contains(';') {
                if is_root_cross_crate_facade(statement) {
                    facades.push(*start);
                }
                pending = None;
            }
            continue;
        }

        let Some(item) = public_item_remainder(trimmed) else {
            continue;
        };
        if !item.starts_with("use ") && !item.starts_with("type ") {
            continue;
        }
        if trimmed.contains(';') {
            if is_root_cross_crate_facade(trimmed) {
                facades.push(line_number);
            }
        } else {
            pending = Some((line_number, trimmed.to_string()));
        }
    }
    facades
}

fn public_rust_function_names(source: &str) -> BTreeSet<String> {
    let masked = rust_code_mask(source);
    masked
        .lines()
        .filter_map(|line| {
            let mut item = public_item_remainder(line)?;
            for qualifier in ["const ", "async ", "unsafe "] {
                if let Some(rest) = item.strip_prefix(qualifier) {
                    item = rest.trim_start();
                }
            }
            let rest = item.strip_prefix("fn ")?;
            let end = rest
                .bytes()
                .position(|byte| !is_identifier_byte(byte))
                .unwrap_or(rest.len());
            (end > 0).then(|| rest[..end].to_string())
        })
        .collect()
}

fn public_top_level_policy_items(source: &str) -> BTreeSet<String> {
    let masked = production_rust_code_mask(source);

    let mut items = BTreeSet::new();
    let mut brace_depth = 0_i32;
    for line in masked.lines() {
        if brace_depth == 0 {
            if let Some(mut item) = public_item_remainder(line) {
                for qualifier in ["unsafe ", "auto "] {
                    if let Some(rest) = item.strip_prefix(qualifier) {
                        item = rest.trim_start();
                    }
                }
                for kind in [
                    "struct", "enum", "union", "trait", "type", "const", "static",
                ] {
                    let Some(rest) = item.strip_prefix(kind).and_then(|rest| {
                        rest.as_bytes()
                            .first()
                            .is_some_and(u8::is_ascii_whitespace)
                            .then(|| rest.trim_start())
                    }) else {
                        continue;
                    };
                    if kind == "const" && rest.starts_with("fn ") {
                        break;
                    }
                    let end = rest
                        .bytes()
                        .position(|byte| !is_identifier_byte(byte))
                        .unwrap_or(rest.len());
                    if end > 0 {
                        items.insert(format!("{kind} {}", &rest[..end]));
                    }
                    break;
                }
            }
        }

        brace_depth += line.bytes().filter(|byte| *byte == b'{').count() as i32;
        brace_depth -= line.bytes().filter(|byte| *byte == b'}').count() as i32;
    }
    items
}

fn production_rust_code_mask(source: &str) -> String {
    let masked = rust_code_mask(source);
    let mut excluded = test_module_ranges(&masked);
    excluded.extend(test_function_ranges(&masked));
    let mut masked_bytes = masked.into_bytes();
    for (start, end) in excluded {
        mask_range(&mut masked_bytes, start, end.saturating_add(1));
    }
    String::from_utf8(masked_bytes).expect("mask preserves UTF-8 byte boundaries")
}

fn owner_ui_marker_is_construction(source: &str, offset: usize, marker: &str) -> bool {
    if !marker.ends_with('{') {
        return true;
    }
    !source[..offset].trim_end().ends_with("->")
}

fn stores_keys_ui_api_items(source: &str) -> (BTreeSet<String>, Vec<(usize, String)>) {
    subject_ui_api_items(source, "keycord_keys")
}

fn subject_ui_api_items(
    source: &str,
    subject_crate: &str,
) -> (BTreeSet<String>, Vec<(usize, String)>) {
    let masked = production_rust_code_mask(source);
    let marker = format!("{subject_crate}::");
    let mut items = BTreeSet::new();
    let mut invalid = Vec::new();

    for (offset, _) in masked.match_indices(&marker) {
        if offset > 0 && is_identifier_byte(masked.as_bytes()[offset - 1]) {
            continue;
        }
        let line = line_number_at(&masked, offset);
        let suffix = &masked[offset + marker.len()..];
        let Some(ui_suffix) = suffix.strip_prefix("ui::") else {
            let reference = suffix
                .bytes()
                .take_while(|byte| is_identifier_byte(*byte) || matches!(*byte, b':' | b'{' | b'}'))
                .map(char::from)
                .collect::<String>();
            invalid.push((line, format!("{subject_crate}::{reference}")));
            continue;
        };

        if let Some(group) = ui_suffix.strip_prefix('{') {
            let Some(close) = group.find('}') else {
                invalid.push((line, format!("{subject_crate}::ui::{{...")));
                continue;
            };
            for entry in group[..close].split(',').map(str::trim) {
                if entry.is_empty() {
                    continue;
                }
                let end = entry
                    .bytes()
                    .position(|byte| !is_identifier_byte(byte))
                    .unwrap_or(entry.len());
                let remainder = entry[end..].trim();
                if end == 0 || (!remainder.is_empty() && !remainder.starts_with("as ")) {
                    invalid.push((line, format!("{subject_crate}::ui::{{{entry}}}")));
                    continue;
                }
                items.insert(entry[..end].to_string());
            }
            continue;
        }

        let end = ui_suffix
            .bytes()
            .position(|byte| !is_identifier_byte(byte))
            .unwrap_or(ui_suffix.len());
        if end == 0 || ui_suffix[end..].starts_with("::") {
            let reference = ui_suffix
                .bytes()
                .take_while(|byte| is_identifier_byte(*byte) || *byte == b':')
                .map(char::from)
                .collect::<String>();
            invalid.push((line, format!("{subject_crate}::ui::{reference}")));
            continue;
        }
        items.insert(ui_suffix[..end].to_string());
    }

    (items, invalid)
}

fn item_has_immediate_cfg_feature(source: &str, marker: &str, feature: &str) -> bool {
    let expected_cfg = format!("#[cfg(feature = \"{feature}\")]");
    let mut previous_nonempty = None;
    for line in source.lines() {
        if line.contains(marker) && previous_nonempty == Some(expected_cfg.as_str()) {
            return true;
        }
        if !line.trim().is_empty() {
            previous_nonempty = Some(line.trim());
        }
    }
    false
}

fn rust_struct_fields(source: &str, name: &str) -> Option<BTreeMap<String, String>> {
    let masked = rust_code_mask(source);
    let marker = format!("struct {name}");
    let start = masked.find(&marker)?;
    let open = start + masked[start..].find('{')?;
    let close = matching_brace(masked.as_bytes(), open)?;
    let mut fields = BTreeMap::new();
    for line in source[open + 1..close].lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let line = public_item_remainder(line).unwrap_or(line);
        let Some((declaration, field_type)) = line.split_once(':') else {
            continue;
        };
        let Some(field) = declaration.split_whitespace().last() else {
            continue;
        };
        if field.is_empty() || !field.bytes().all(is_identifier_byte) {
            continue;
        }
        let field_type = field_type.trim().trim_end_matches(',').trim();
        fields.insert(field.to_string(), field_type.to_string());
    }
    Some(fields)
}

fn valid_subject_name(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
}

fn valid_action_pattern(pattern: &str) -> bool {
    let wildcard_count = pattern.bytes().filter(|byte| *byte == b'*').count();
    if wildcard_count > 1 || (wildcard_count == 1 && !pattern.ends_with('*')) {
        return false;
    }
    let stem = pattern.strip_suffix('*').unwrap_or(pattern);
    !stem.is_empty()
        && stem
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && stem.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && (pattern.ends_with('*') || !stem.ends_with('-'))
}

fn action_patterns_overlap(left: &str, right: &str) -> bool {
    let left_wildcard = left.ends_with('*');
    let right_wildcard = right.ends_with('*');
    let left_stem = left.strip_suffix('*').unwrap_or(left);
    let right_stem = right.strip_suffix('*').unwrap_or(right);
    match (left_wildcard, right_wildcard) {
        (false, false) => left == right,
        (true, false) => right.starts_with(left_stem),
        (false, true) => left.starts_with(right_stem),
        (true, true) => left_stem.starts_with(right_stem) || right_stem.starts_with(left_stem),
    }
}

fn subject_name(directory: &Path) -> Option<String> {
    directory
        .file_name()?
        .to_str()?
        .strip_prefix("keycord-")
        .filter(|subject| valid_subject_name(subject))
        .map(str::to_string)
}

fn is_architecture_support_crate(subject: &str) -> bool {
    matches!(subject, "architecture" | "ui-fragments")
}

fn is_test_path(path: &Path) -> bool {
    path.file_name().and_then(|name| name.to_str()) == Some("tests.rs")
        || path
            .components()
            .any(|component| component.as_os_str() == "tests")
}

fn ui_window_action_mentions(source: &str) -> Vec<WindowActionMention> {
    let mut values = extract_element_values(source, "property", "action-name");
    values.extend(extract_element_values(source, "attribute", "action"));
    values
        .into_iter()
        .filter_map(|value| {
            let action = normalized_qualified_window_action(&value)?.to_string();
            let offset = source.find(&value).unwrap_or_default();
            Some(WindowActionMention {
                action,
                line: line_number_at(source, offset),
                declaration_required: true,
                registration: false,
            })
        })
        .collect()
}

fn rust_window_action_mentions(
    source: &str,
    policy: &WindowActionPolicy,
) -> Vec<WindowActionMention> {
    let literals = production_rust_string_literals(source);
    let registration_literals = window_action_registration_literal_starts(source, &literals);
    let mut mentions: BTreeMap<String, WindowActionMention> = BTreeMap::new();
    for literal in literals {
        let registration = registration_literals.contains(&literal.start);
        let (action, qualified) =
            if let Some(action) = normalized_qualified_window_action(&literal.value) {
                (action, true)
            } else {
                (literal.value.as_str(), false)
            };
        if action.is_empty() || (!qualified && !registration && policy.rule_for(action).is_none()) {
            continue;
        }
        let mention = WindowActionMention {
            action: action.to_string(),
            line: line_number_at(source, literal.start),
            declaration_required: qualified || registration,
            registration,
        };
        mentions
            .entry(mention.action.clone())
            .and_modify(|existing| {
                existing.declaration_required |= mention.declaration_required;
                existing.registration |= mention.registration;
                existing.line = existing.line.min(mention.line);
            })
            .or_insert(mention);
    }
    mentions.into_values().collect()
}

fn normalized_qualified_window_action(value: &str) -> Option<&str> {
    let action = value.strip_prefix("win.")?;
    (!action.is_empty()
        && action.bytes().all(|byte| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || matches!(byte, b'-' | b'_' | b'{' | b'}')
        }))
    .then_some(action)
}

fn line_number_at(source: &str, offset: usize) -> usize {
    source.as_bytes()[..offset.min(source.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
}

fn production_rust_string_literals(source: &str) -> Vec<RustStringLiteral> {
    let mask = rust_code_mask(source);
    let mut excluded = test_module_ranges(&mask);
    excluded.extend(test_function_ranges(&mask));
    rust_string_literals(source)
        .into_iter()
        .filter(|literal| {
            !excluded
                .iter()
                .any(|(start, end)| literal.start >= *start && literal.start <= *end)
        })
        .collect()
}

fn rust_string_literals(source: &str) -> Vec<RustStringLiteral> {
    let bytes = source.as_bytes();
    let mut literals = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if index + 1 < bytes.len() && &bytes[index..index + 2] == b"//" {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
        } else if index + 1 < bytes.len() && &bytes[index..index + 2] == b"/*" {
            index += 2;
            let mut depth = 1;
            while index < bytes.len() && depth > 0 {
                if index + 1 < bytes.len() && &bytes[index..index + 2] == b"/*" {
                    depth += 1;
                    index += 2;
                } else if index + 1 < bytes.len() && &bytes[index..index + 2] == b"*/" {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if let Some((content_start, content_end, end)) =
            raw_string_literal_parts(bytes, index)
        {
            literals.push(RustStringLiteral {
                value: source[content_start..content_end].to_string(),
                start: index,
            });
            index = end;
        } else if bytes[index] == b'"' {
            let start = index;
            let content_start = index + 1;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'"' {
                    literals.push(RustStringLiteral {
                        value: source[content_start..index].to_string(),
                        start,
                    });
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else if bytes[index] == b'\'' {
            index = char_literal_end(bytes, index).unwrap_or(index + 1);
        } else {
            index += 1;
        }
    }
    literals
}

fn window_action_registration_literal_starts(
    source: &str,
    literals: &[RustStringLiteral],
) -> BTreeSet<usize> {
    let mask = rust_code_mask(source);
    let bytes = mask.as_bytes();
    let marker = "register_window_action";
    let mut starts = BTreeSet::new();
    for (offset, _) in mask.match_indices(marker) {
        let marker_end = offset + marker.len();
        if (offset > 0 && is_identifier_byte(bytes[offset - 1]))
            || bytes
                .get(marker_end)
                .is_some_and(|byte| is_identifier_byte(*byte))
        {
            continue;
        }
        let mut cursor = marker_end;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'(') {
            continue;
        }
        let Some((argument_start, argument_end)) = second_call_argument_range(bytes, cursor) else {
            continue;
        };
        starts.extend(
            literals
                .iter()
                .filter(|literal| literal.start >= argument_start && literal.start < argument_end)
                .map(|literal| literal.start),
        );
    }
    starts
}

fn second_call_argument_range(bytes: &[u8], open: usize) -> Option<(usize, usize)> {
    let mut parentheses = 1_i32;
    let mut brackets = 0_i32;
    let mut braces = 0_i32;
    let mut argument_start = None;
    for (index, byte) in bytes.iter().enumerate().skip(open + 1) {
        match byte {
            b'(' => parentheses += 1,
            b')' => {
                parentheses -= 1;
                if parentheses == 0 {
                    return argument_start.map(|start| (start, index));
                }
            }
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'{' => braces += 1,
            b'}' => braces -= 1,
            b',' if parentheses == 1 && brackets == 0 && braces == 0 => {
                if argument_start.is_some() {
                    return argument_start.map(|start| (start, index));
                }
                argument_start = Some(index + 1);
            }
            _ => {}
        }
    }
    None
}

fn is_root_catchall(relative: &str) -> bool {
    relative == "src/support.rs"
        || relative.starts_with("src/support/")
        || relative == "src/tools.rs"
        || relative.starts_with("src/tools/")
        || relative == "src/window/tools.rs"
        || relative.starts_with("src/window/tools/")
}

fn is_forbidden_root_compatibility_path(relative: &str) -> bool {
    relative == "src/logging.rs"
        || relative.starts_with("src/logging/")
        || relative == "src/preferences.rs"
        || relative.starts_with("src/preferences/")
        || relative == "src/private_key.rs"
        || relative.starts_with("src/private_key/")
        || matches!(relative, "src/filters.rs" | "src/qr_code.rs")
}

fn collect_production_rust(
    directory: &Path,
    files: &mut Vec<PathBuf>,
    violations: &mut Vec<String>,
) {
    match recursive_files(directory) {
        Ok(found) => files.extend(found.into_iter().filter(|path| {
            path.extension().and_then(|extension| extension.to_str()) == Some("rs")
                && path.file_name().and_then(|name| name.to_str()) != Some("tests.rs")
                && !path
                    .components()
                    .any(|component| component.as_os_str() == "tests")
        })),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => violations.push(format!("{}: {error}", directory.display())),
    }
}

fn substantial(source: &str, minimum_lines: usize, minimum_bytes: usize) -> bool {
    source.len() >= minimum_bytes
        && source
            .lines()
            .filter(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with("//")
            })
            .count()
            >= minimum_lines
}

fn is_trivial_struct_literal_adapter(body: &str) -> bool {
    let masked = rust_code_mask(body);
    let trimmed = masked.trim();
    let Some(open) = trimmed.find('{') else {
        return false;
    };
    let constructor = trimmed[..open].trim();
    let constructor_is_type = constructor == "Self"
        || constructor
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_uppercase);
    constructor_is_type
        && trimmed.ends_with('}')
        && !trimmed.contains(';')
        && !trimmed.contains(" if ")
        && !trimmed.contains("match ")
        && !trimmed.contains("loop ")
        && !trimmed.contains("while ")
        && !trimmed.contains("for ")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RustFunction {
    name: String,
    body: String,
}

fn rust_functions(source: &str) -> Vec<RustFunction> {
    let mask = rust_code_mask(source);
    let test_ranges = test_module_ranges(&mask);
    let bytes = mask.as_bytes();
    let mut functions = Vec::new();
    let mut index = 0;

    while index + 2 <= bytes.len() {
        if &bytes[index..index + 2] != b"fn"
            || (index > 0 && is_identifier_byte(bytes[index - 1]))
            || (index + 2 < bytes.len() && is_identifier_byte(bytes[index + 2]))
        {
            index += 1;
            continue;
        }
        if test_ranges
            .iter()
            .any(|(start, end)| index >= *start && index <= *end)
            || has_test_attribute(&mask, index)
        {
            index += 2;
            continue;
        }
        let mut cursor = index + 2;
        skip_ascii_whitespace(bytes, &mut cursor);
        let name_start = cursor;
        while cursor < bytes.len() && is_identifier_byte(bytes[cursor]) {
            cursor += 1;
        }
        if name_start == cursor {
            index += 2;
            continue;
        }
        let name = mask[name_start..cursor].to_string();
        let Some(open) = find_function_open(bytes, cursor) else {
            index += 2;
            continue;
        };
        let Some(close) = matching_brace(bytes, open) else {
            index += 2;
            continue;
        };
        functions.push(RustFunction {
            name,
            body: source[open + 1..close].to_string(),
        });
        index = close + 1;
    }
    functions
}

fn rust_code_mask(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut masked = bytes.to_vec();
    let mut index = 0;
    while index < bytes.len() {
        if index + 1 < bytes.len() && &bytes[index..index + 2] == b"//" {
            let start = index;
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            mask_range(&mut masked, start, index);
        } else if index + 1 < bytes.len() && &bytes[index..index + 2] == b"/*" {
            let start = index;
            index += 2;
            let mut depth = 1;
            while index < bytes.len() && depth > 0 {
                if index + 1 < bytes.len() && &bytes[index..index + 2] == b"/*" {
                    depth += 1;
                    index += 2;
                } else if index + 1 < bytes.len() && &bytes[index..index + 2] == b"*/" {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            mask_range(&mut masked, start, index);
        } else if let Some((content_start, end)) = raw_string_bounds(bytes, index) {
            mask_range(&mut masked, content_start, end);
            index = end;
        } else if bytes[index] == b'"' {
            let start = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'"' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            mask_range(&mut masked, start, index);
        } else if bytes[index] == b'\'' {
            if let Some(end) = char_literal_end(bytes, index) {
                mask_range(&mut masked, index, end);
                index = end;
            } else {
                index += 1;
            }
        } else {
            index += 1;
        }
    }
    String::from_utf8(masked).expect("mask preserves UTF-8 byte boundaries")
}

fn raw_string_bounds(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    raw_string_literal_parts(bytes, index).map(|(_, _, end)| (index, end))
}

fn raw_string_literal_parts(bytes: &[u8], index: usize) -> Option<(usize, usize, usize)> {
    let mut cursor = index;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    cursor += 1;
    let hashes_start = cursor;
    while bytes.get(cursor) == Some(&b'#') {
        cursor += 1;
    }
    let hash_count = cursor - hashes_start;
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    let content_start = cursor + 1;
    cursor += 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes.get(cursor + 1..cursor + 1 + hash_count)
                == Some(&bytes[hashes_start..hashes_start + hash_count])
        {
            return Some((content_start, cursor, cursor + 1 + hash_count));
        }
        cursor += 1;
    }
    Some((content_start, bytes.len(), bytes.len()))
}

fn char_literal_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start + 1;
    if bytes.get(cursor) == Some(&b'\\') {
        cursor += 2;
        if bytes.get(cursor.saturating_sub(1)) == Some(&b'u') && bytes.get(cursor) == Some(&b'{') {
            while cursor < bytes.len() && bytes[cursor] != b'}' {
                cursor += 1;
            }
            cursor += usize::from(cursor < bytes.len());
        }
    } else {
        let character = source_character_width(*bytes.get(cursor)?);
        cursor += character;
    }
    (bytes.get(cursor) == Some(&b'\'')).then_some(cursor + 1)
}

fn source_character_width(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first & 0xe0 == 0xc0 {
        2
    } else if first & 0xf0 == 0xe0 {
        3
    } else {
        4
    }
}

fn mask_range(masked: &mut [u8], start: usize, end: usize) {
    let end = end.min(masked.len());
    for byte in &mut masked[start..end] {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
}

fn test_module_ranges(mask: &str) -> Vec<(usize, usize)> {
    let bytes = mask.as_bytes();
    let mut ranges = Vec::new();
    let mut search_from = 0;
    while let Some(offset) = mask[search_from..].find("mod tests") {
        let module = search_from + offset;
        let context_start = mask[..module]
            .rfind(['}', ';'])
            .map_or(0, |boundary| boundary + 1);
        if !mask[context_start..module].contains("cfg(test") {
            search_from = module + "mod tests".len();
            continue;
        }
        let Some(open_offset) = mask[module..].find('{') else {
            break;
        };
        let open = module + open_offset;
        if let Some(close) = matching_brace(bytes, open) {
            ranges.push((context_start, close));
            search_from = close + 1;
        } else {
            break;
        }
    }
    ranges
}

fn test_function_ranges(mask: &str) -> Vec<(usize, usize)> {
    let bytes = mask.as_bytes();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index + 2 <= bytes.len() {
        if &bytes[index..index + 2] != b"fn"
            || (index > 0 && is_identifier_byte(bytes[index - 1]))
            || (index + 2 < bytes.len() && is_identifier_byte(bytes[index + 2]))
            || !has_test_attribute(mask, index)
        {
            index += 1;
            continue;
        }
        let context_start = mask[..index]
            .rfind(['}', ';'])
            .map_or(0, |boundary| boundary + 1);
        let Some(open) = find_function_open(bytes, index + 2) else {
            index += 2;
            continue;
        };
        let Some(close) = matching_brace(bytes, open) else {
            index += 2;
            continue;
        };
        ranges.push((context_start, close));
        index = close + 1;
    }
    ranges
}

fn has_test_attribute(mask: &str, function: usize) -> bool {
    let context_start = mask[..function]
        .rfind(['}', ';'])
        .map_or(0, |boundary| boundary + 1);
    let context = &mask[context_start..function];
    context.contains("#[test]") || context.contains("#[cfg(test")
}

fn find_function_open(bytes: &[u8], mut cursor: usize) -> Option<usize> {
    let mut parentheses = 0_i32;
    let mut brackets = 0_i32;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'(' => parentheses += 1,
            b')' => parentheses -= 1,
            b'[' => brackets += 1,
            b']' => brackets -= 1,
            b'{' if parentheses == 0 && brackets == 0 => return Some(cursor),
            b';' if parentheses == 0 && brackets == 0 => return None,
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn matching_brace(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0_i32;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match byte {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn skip_ascii_whitespace(bytes: &[u8], cursor: &mut usize) {
    while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
        *cursor += 1;
    }
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn child_directories(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut directories = fs::read_dir(path)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    directories.sort();
    Ok(directories)
}

fn recursive_files(path: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let mut entries = fs::read_dir(&directory)?.collect::<Result<Vec<_>, _>>()?;
        entries.sort_by_key(fs::DirEntry::path);
        for entry in entries.into_iter().rev() {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_parser_handles_multiline_values_and_target_dependencies() {
        let manifest = Manifest::parse(
            r#"
[features]
default = [
]

[target.'cfg(target_os = "linux")'.dependencies]
keycord-fido = { workspace = true, default-features = false }
"#,
        );

        assert_eq!(
            manifest.value("features", "default").map(compact),
            Some("[]".into())
        );
        assert_eq!(
            manifest
                .normal_dependency_assignments()
                .map(|assignment| dependency_name(&assignment.key))
                .collect::<Vec<_>>(),
            vec!["keycord-fido"]
        );
    }

    #[test]
    fn lifecycle_build_dependency_is_not_a_normal_edge() {
        let manifest = Manifest::parse(
            r#"
[dependencies]
keycord-runtime.workspace = true
[build-dependencies]
keycord-passkey.workspace = true
"#,
        );

        assert_eq!(manifest.normal_dependency_assignments().count(), 1);
        assert_eq!(manifest.dependency_assignments().count(), 2);
    }

    #[test]
    fn forbidden_subject_edges_are_reported() {
        let mut checker = Checker::default();
        let fido = Manifest::parse(
            r#"
[dependencies]
keycord-passkey.workspace = true
"#,
        );
        checker.check_forbidden_edges("fido/Cargo.toml", "keycord-fido", &fido);

        let lifecycle = Manifest::parse(
            r#"
[dependencies]
keycord-passkey.workspace = true
"#,
        );
        checker.check_forbidden_edges("lifecycle/Cargo.toml", "keycord-lifecycle", &lifecycle);

        assert_eq!(checker.violations.len(), 2);
        assert!(checker.violations[0].contains("separate subjects"));
        assert!(checker.violations[1].contains("composition root"));
    }

    #[test]
    fn workspace_internal_dependencies_must_disable_defaults() {
        let manifest = Manifest::parse(
            r#"
[workspace.dependencies]
keycord-good = { path = "good", default-features = false }
keycord-bad = { path = "bad" }
external = "1"
"#,
        );
        let mut checker = Checker::default();
        checker.check_workspace_dependency_defaults(&manifest);

        assert_eq!(checker.violations.len(), 1);
        assert!(checker.violations[0].contains("keycord-bad"));
    }

    #[test]
    fn root_cross_crate_facades_are_detected() {
        assert!(is_root_cross_crate_facade(
            "pub use keycord_git::ui::clone_store_repository;"
        ));
        assert!(is_root_cross_crate_facade(
            "pub(crate) type DirtyProbe = keycord_lifecycle::updater::DirtyProbe;"
        ));
        assert!(!is_root_cross_crate_facade(
            "use keycord_git::ui::clone_store_repository;"
        ));
        assert!(!is_root_cross_crate_facade(
            "pub use self::widgets::Widgets;"
        ));
        assert_eq!(
            root_cross_crate_facade_lines(
                "pub type DirtyProbe =\n    keycord_lifecycle::updater::DirtyProbe;\n"
            ),
            vec![1]
        );
    }

    #[test]
    fn public_function_inventory_ignores_private_and_commented_functions() {
        let source = r#"
pub fn generic_capability() {}
pub const fn compile_time_capability() -> bool { true }
fn private_probe() {}
// pub fn commented_out() {}
"#;
        assert_eq!(
            public_rust_function_names(source),
            BTreeSet::from([
                "compile_time_capability".to_string(),
                "generic_capability".to_string(),
            ])
        );
    }

    #[test]
    fn runtime_policy_inventory_rejects_subject_constants_and_types() {
        let source = r#"
pub struct TomlParseLimits {
    pub max_bytes: usize,
}

impl TomlParseLimits {
    pub const fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }
}

pub const PREFERENCE_FILE_TOML_LIMITS: TomlParseLimits = TomlParseLimits::new(64);
pub type FidoEnvelopeLimits = TomlParseLimits;

#[cfg(test)]
mod tests {
    pub const TEST_LIMITS: TomlParseLimits = TomlParseLimits::new(1);
}
"#;

        assert_eq!(
            public_top_level_policy_items(source),
            BTreeSet::from([
                "const PREFERENCE_FILE_TOML_LIMITS".to_string(),
                "struct TomlParseLimits".to_string(),
                "type FidoEnvelopeLimits".to_string(),
            ])
        );
    }

    #[test]
    fn runtime_subject_identifier_policy_names_known_leaks() {
        let policy = policy_lines(include_str!(
            "../policy/runtime-forbidden-subject-identifiers.txt"
        ));

        assert!(policy.contains("PASSWORD_STORE_"));
        assert!(policy.contains("PREFERENCE_FILE_TOML_LIMITS"));
        assert!(policy.contains("MANAGED_KEY_MANIFEST_TOML_LIMITS"));
        assert!(policy.contains("FIDO2_TEXT_ENVELOPE_TOML_LIMITS"));
    }

    #[test]
    fn root_owner_ui_policy_names_retired_construction_seams() {
        let policy = policy_lines(include_str!(
            "../policy/root-forbidden-owner-ui-construction.txt"
        ));

        assert!(policy.contains("PreferencesPageSearchState::new"));
        assert!(policy.contains("SearchablePreferencesGroup::"));
        assert!(policy.contains("StoreGitPageState {"));
        assert!(policy.contains("StoreRecipientsPlatformState {"));
        assert!(policy.contains("StoreImportPageWidgets"));
        assert!(policy.contains("PasswordPageState {"));
        assert!(policy.contains("DocumentationPageWidgets"));
        assert!(policy.contains("GitActionWidgets"));
    }

    #[test]
    fn owner_ui_construction_markers_ignore_function_return_types() {
        let declaration = "fn state() -> PasswordPageState { todo!() }";
        let declaration_offset = declaration.find("PasswordPageState {").unwrap();
        assert!(!owner_ui_marker_is_construction(
            declaration,
            declaration_offset,
            "PasswordPageState {"
        ));

        let construction = "let state = PasswordPageState { nav };";
        let construction_offset = construction.find("PasswordPageState {").unwrap();
        assert!(owner_ui_marker_is_construction(
            construction,
            construction_offset,
            "PasswordPageState {"
        ));
    }

    #[test]
    fn stores_recipient_ui_is_limited_to_reviewed_keys_controller_items() {
        let source = r#"
use keycord_keys::ui::{
    KeyManagementUiState,
    RecipientKeyListContext as Context,
};
use keycord_keys::ManagedRipassoPrivateKey;
use keycord_keys::ui::recipient_list::private_helper;

#[cfg(test)]
mod tests {
    use keycord_keys::ManagedRipassoPrivateKey;
}
"#;
        let (items, invalid) = stores_keys_ui_api_items(source);

        assert_eq!(
            items,
            BTreeSet::from([
                "KeyManagementUiState".to_string(),
                "RecipientKeyListContext".to_string(),
            ])
        );
        assert_eq!(invalid.len(), 2);
        assert!(invalid
            .iter()
            .any(|(_, reference)| reference == "keycord_keys::ManagedRipassoPrivateKey"));
        assert!(invalid.iter().any(|(_, reference)| {
            reference == "keycord_keys::ui::recipient_list::private_helper"
        }));
    }

    #[test]
    fn keys_ui_is_limited_to_reviewed_fido_presentation_items() {
        let source = r#"
use keycord_fido::ui::{
    FidoWindowWidgets,
    FidoKeyGenerationUiPorts as GenerationPorts,
};
use keycord_fido::FidoService;
use keycord_fido::ui::private::start_key_generation;
"#;
        let (items, invalid) = subject_ui_api_items(source, "keycord_fido");

        assert_eq!(
            items,
            BTreeSet::from([
                "FidoKeyGenerationUiPorts".to_string(),
                "FidoWindowWidgets".to_string(),
            ])
        );
        assert_eq!(invalid.len(), 2);
        assert!(invalid
            .iter()
            .any(|(_, reference)| reference == "keycord_fido::FidoService"));
        assert!(invalid.iter().any(|(_, reference)| {
            reference == "keycord_fido::ui::private::start_key_generation"
        }));
    }

    #[test]
    fn fido_bundle_composition_is_feature_gated() {
        let source = r#"
#[cfg(feature = "fidokey")]
use keycord_fido::ui::FidoWindowWidgets;

struct Ungated {
    fido: FidoWindowWidgets,
}
"#;
        assert!(item_has_immediate_cfg_feature(
            source,
            "use keycord_fido::ui::FidoWindowWidgets;",
            "fidokey"
        ));
        assert!(!item_has_immediate_cfg_feature(
            source,
            "fido: FidoWindowWidgets,",
            "fidokey"
        ));
    }

    #[test]
    fn fido_ownership_policies_lock_ui_and_service_lifecycle() {
        let recipient_owners =
            named_inventory(include_str!("../policy/recipient-ui-id-owners.txt"));
        assert_eq!(
            recipient_owners
                .get("store_recipients_generate_fido2_key_row")
                .map(String::as_str),
            Some("fido")
        );

        let root_bundles =
            named_inventory(include_str!("../policy/root-window-widget-bundles.txt"));
        assert_eq!(
            root_bundles.get("fido").map(String::as_str),
            Some("FidoWindowWidgets")
        );

        let lifecycle = policy_lines(include_str!(
            "../policy/keys-forbidden-fido-service-lifecycle.txt"
        ));
        for marker in [
            "OnceLock",
            "RwLock",
            "FidoService::native",
            "FidoService::new",
            "set_shared_native_transport_for_tests",
            "reset_shared_native_transport_for_tests",
        ] {
            assert!(
                lifecycle.contains(marker),
                "missing lifecycle marker {marker}"
            );
        }
    }

    #[test]
    fn root_widget_registry_extracts_bundle_fields() {
        let source = r#"
pub(in crate::window) struct WindowWidgets {
    pub(in crate::window) entries: EntryWindowWidgets,
    pub(in crate::window) shell: ShellWindowWidgets,
}
"#;
        assert_eq!(
            rust_struct_fields(source, "WindowWidgets"),
            Some(BTreeMap::from([
                ("entries".to_string(), "EntryWindowWidgets".to_string()),
                ("shell".to_string(), "ShellWindowWidgets".to_string()),
            ]))
        );
    }

    #[test]
    fn fragment_identity_requires_subject_ownership() {
        assert_eq!(
            fragment_identity("crates/keycord-git/data/window-busy-page.fragment.ui"),
            Some((FragmentKind::Window, "git-busy-page".to_string()))
        );
        assert_eq!(
            fragment_identity("crates/keycord-docs/data/shortcuts-tool-item.fragment.ui"),
            Some((FragmentKind::Shortcuts, "docs-tool-item".to_string()))
        );
        assert_eq!(
            fragment_identity("crates/keycord-shell/data/window-subject.fragment.ui"),
            None
        );
        assert_eq!(
            fragment_identity("crates/keycord-docs/data/menu-tool-item.fragment.ui"),
            None
        );
        assert_eq!(
            fragment_identity("crates/keycord-docs/data/shortcuts-.fragment.ui"),
            None
        );
    }

    #[test]
    fn generated_root_data_is_not_treated_as_canonical_ui() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "keycord-architecture-generated-data-{}-{unique}",
            std::process::id()
        ));
        let generated = root.join("data/window-page.fragment.ui");
        let forbidden = root.join("src/window.ui");
        fs::create_dir_all(generated.parent().unwrap()).unwrap();
        fs::create_dir_all(forbidden.parent().unwrap()).unwrap();
        fs::write(generated, "<object />").unwrap();
        fs::write(&forbidden, "<object />").unwrap();

        let mut checker = Checker::default();
        checker.check_no_root_declarative_ui(&root);

        assert_eq!(checker.violations.len(), 1);
        assert!(checker.violations[0].starts_with("src/window.ui:"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fragment_registry_pairs_names_and_paths() {
        let source = r#"
const FRAGMENTS: &[(&str, &str)] = &[
    ("one", "crates/keycord-one/data/window-page.fragment.ui"),
    (
        "two",
        "crates/keycord-two/data/window-page.fragment.ui",
    ),
];
"#;
        let registry = fragment_registry(source, "FRAGMENTS")
            .unwrap()
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            registry.get("one").map(String::as_str),
            Some("crates/keycord-one/data/window-page.fragment.ui")
        );
        assert_eq!(
            registry.get("two").map(String::as_str),
            Some("crates/keycord-two/data/window-page.fragment.ui")
        );
    }

    #[test]
    fn shell_inventory_extractors_ignore_marker_names() {
        let source = r#"
<object id="shell_id">
  <!-- keycord-window-fragment:git-page -->
  <property name="action-name">win.generic</property>
  <property name="title" translatable="yes">Generic</property>
</object>
"#;
        let without_markers = source
            .lines()
            .filter(|line| !line.contains("keycord-window-fragment:"))
            .collect::<Vec<_>>()
            .join("\n");
        assert_eq!(
            extract_after(&without_markers, "id=\""),
            BTreeSet::from(["shell_id".to_string()])
        );
        assert_eq!(
            extract_element_values(&without_markers, "property", "action-name"),
            BTreeSet::from(["win.generic".to_string()])
        );
        assert_eq!(
            extract_translatable_values(&without_markers),
            BTreeSet::from(["Generic".to_string()])
        );
    }

    #[test]
    fn window_action_policy_blocks_peer_and_root_actions() {
        let policy =
            WindowActionPolicy::parse(include_str!("../policy/window-action-owners.txt")).unwrap();
        let git_foreign_actions = [
            "context-save",
            "context-undo",
            "open-new-password",
            "toggle-find",
            "open-raw-pass-file",
            "save-password",
            "save-store-recipients",
            "open-preferences",
            "open-tools",
            "open-docs",
            "toggle-hidden-and-duplicates",
            "open-store-recipients-{slot}",
            "reload-store-recipients-list",
            "reload-password-list",
        ];
        for action in git_foreign_actions {
            let rule = policy.rule_for(action).unwrap();
            assert!(!rule.allows("git"), "Git unexpectedly permits {action}");
        }
        for action in [
            "git-clone",
            "open-git",
            "open-store-git-{slot}",
            "open-store-git-1",
            "synchronize",
        ] {
            assert!(
                policy.rule_for(action).unwrap().allows("git"),
                "Git must own {action}"
            );
        }
        for action in ["back", "go-home", "toggle-find"] {
            assert!(
                policy.rule_for(action).unwrap().allows("entries"),
                "Entries must be allowed to use {action}"
            );
        }
        assert!(!policy
            .rule_for("open-preferences")
            .unwrap()
            .allows("entries"));
    }

    #[test]
    fn window_action_scanner_ignores_comments_and_test_code() {
        let policy = WindowActionPolicy::parse(
            r#"
git open-git
git open-store-git-*
preferences open-preferences
"#,
        )
        .unwrap();
        let source = r#"
// "win.open-preferences"
const OWN_REFERENCE: &str = "win.open-git";
const FOREIGN_REFERENCE: &str = "open-preferences";

fn register(window: &Window, slot: usize) {
    register_window_action(
        window,
        &format!("open-store-git-{slot}"),
        || {},
    );
}

#[test]
fn ignored_test() {
    let _ = "win.open-preferences";
}

#[cfg(test)]
mod tests {
    const ALSO_IGNORED: &str = "win.open-preferences";
}
"#;
        let mentions = rust_window_action_mentions(source, &policy)
            .into_iter()
            .map(|mention| {
                (
                    mention.action,
                    mention.declaration_required,
                    mention.registration,
                )
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(
            mentions,
            BTreeSet::from([
                ("open-git".to_string(), true, false),
                ("open-preferences".to_string(), false, false),
                ("open-store-git-{slot}".to_string(), true, true),
            ])
        );
    }

    #[test]
    fn window_action_policy_rejects_ambiguous_patterns() {
        let errors = WindowActionPolicy::parse(
            r#"
stores open-store-*
git open-store-git-*
"#,
        )
        .unwrap_err();

        assert!(errors
            .iter()
            .any(|error| error.contains("overlapping window-action patterns")));
    }

    #[test]
    fn exact_function_extraction_ignores_tests_and_braces_in_literals() {
        let source = r##"
fn production(value: bool) {
    let braces = r#"{ not code }"#;
    if value {
        first();
        second();
        third();
    }
}

#[test]
fn ignored_test() {
    first();
    second();
}

#[cfg(test)]
mod tests {
    fn helper() {
        first();
        second();
    }
}
"##;
        let functions = rust_functions(source);
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "production");
        assert!(functions[0].body.contains("{ not code }"));
    }

    #[test]
    fn catchall_policy_is_narrow() {
        assert!(is_root_catchall("src/support/ui.rs"));
        assert!(is_root_catchall("src/support.rs"));
        assert!(is_root_catchall("src/window/tools/menu.rs"));
        assert!(is_root_catchall("src/window/tools.rs"));
        assert!(is_root_catchall("src/tools.rs"));
        assert!(!is_root_catchall("src/composition/git_audit.rs"));
        assert!(!is_root_catchall("crates/keycord-stores/src/support.rs"));
    }

    #[test]
    fn duplicate_detector_uses_exact_substantial_bodies() {
        let body = r#"
fn first() {
    alpha();
    beta();
    gamma();
    delta();
    epsilon();
    zeta();
    eta();
    theta();
    iota();
}
fn second() {
    alpha();
    beta();
    gamma();
    delta();
    epsilon();
    zeta();
    eta();
    theta();
    iota();
}
"#;
        let functions = rust_functions(body);
        assert_eq!(functions.len(), 2);
        assert_eq!(functions[0].body, functions[1].body);
    }

    #[test]
    fn field_only_struct_adapters_are_not_treated_as_implementations() {
        let body = r#"
            WindowChrome {
                back: &self.back,
                add: &self.add,
                find: &self.find,
                primary_action: &self.primary_action,
                secondary_action: &self.secondary_action,
                save: &self.save,
                raw: &self.raw,
                title: &self.title,
            }
        "#;
        assert!(is_trivial_struct_literal_adapter(body));
    }
}
