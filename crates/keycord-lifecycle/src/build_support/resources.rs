//! Shell window-fragment composition and application resource compilation.

use super::write_if_changed;
use keycord_ui_fragments::compose_marked_fragments;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const WINDOW_UI_FRAGMENT_NAMESPACE: &str = "keycord-window-fragment";

const WINDOW_UI_FRAGMENTS: &[(&str, &str)] = &[
    (
        "entries-header-list",
        "crates/keycord-entries/data/window-header-list.fragment.ui",
    ),
    (
        "git-header-audit-filter",
        "crates/keycord-git/data/window-header-audit-filter.fragment.ui",
    ),
    (
        "entries-header-editor",
        "crates/keycord-entries/data/window-header-editor.fragment.ui",
    ),
    (
        "git-header-action",
        "crates/keycord-git/data/window-header-action.fragment.ui",
    ),
    (
        "stores-header-action",
        "crates/keycord-stores/data/window-header-action.fragment.ui",
    ),
    (
        "entries-pages",
        "crates/keycord-entries/data/window-pages.fragment.ui",
    ),
    (
        "preferences-page",
        "crates/keycord-preferences/data/window-page.fragment.ui",
    ),
    (
        "entries-tool-rows",
        "crates/keycord-entries/data/window-tool-rows.fragment.ui",
    ),
    (
        "git-tool-row",
        "crates/keycord-git/data/window-tool-row.fragment.ui",
    ),
    (
        "docs-tool-row",
        "crates/keycord-docs/data/window-tool-row.fragment.ui",
    ),
    (
        "docs-pages",
        "crates/keycord-docs/data/window-pages.fragment.ui",
    ),
    (
        "entries-tool-pages",
        "crates/keycord-entries/data/window-tool-pages.fragment.ui",
    ),
    (
        "git-audit-page",
        "crates/keycord-git/data/window-audit-page.fragment.ui",
    ),
    (
        "stores-pages",
        "crates/keycord-stores/data/window-pages.fragment.ui",
    ),
    (
        "keys-recipient-warning",
        "crates/keycord-keys/data/window-recipient-warning.fragment.ui",
    ),
    (
        "keys-recipient-actions",
        "crates/keycord-keys/data/window-recipient-actions.fragment.ui",
    ),
    (
        "fido-generation-row",
        "crates/keycord-fido/data/window-generation-row.fragment.ui",
    ),
    (
        "git-store-page",
        "crates/keycord-git/data/window-store-page.fragment.ui",
    ),
    (
        "keys-generation-pages",
        "crates/keycord-keys/data/window-generation-pages.fragment.ui",
    ),
    (
        "git-busy-page",
        "crates/keycord-git/data/window-busy-page.fragment.ui",
    ),
    (
        "entries-menu-items",
        "crates/keycord-entries/data/window-menu-items.fragment.ui",
    ),
    (
        "git-menu-item",
        "crates/keycord-git/data/window-menu-item.fragment.ui",
    ),
    (
        "preferences-menu-item",
        "crates/keycord-preferences/data/window-menu-item.fragment.ui",
    ),
    (
        "lifecycle-menu-item",
        "crates/keycord-lifecycle/data/window-menu-item.fragment.ui",
    ),
];

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct ResourceFileEntry {
    source: String,
    alias: Option<String>,
}

impl ResourceFileEntry {
    fn source(source: String) -> Self {
        Self {
            source,
            alias: None,
        }
    }
}

pub(super) fn build_resources(source_root: &Path, resource_id: &str) {
    let data_dir = source_root.join("data");
    fs::create_dir_all(&data_dir).expect("Failed to create data directory");
    let branding_dir = source_root.join("crates/keycord-lifecycle/data/branding");
    let git_data_dir = source_root.join("crates/keycord-git/data");
    let shell_data_dir = source_root.join("crates/keycord-shell/data");

    let mut resource_files = Vec::new();
    collect_icon_assets(&branding_dir, &branding_dir, &mut resource_files);
    collect_icon_assets(&git_data_dir, &git_data_dir, &mut resource_files);
    collect_icon_assets(&shell_data_dir, &shell_data_dir, &mut resource_files);
    resource_files.sort();
    let resources_xml = write_resources_xml(&data_dir, resource_id, &resource_files);

    let current_dir = std::env::current_dir().expect("Failed to read build working directory");
    if current_dir == source_root {
        glib_build_tools::compile_resources(
            &[
                Path::new("crates/keycord-lifecycle/data/branding"),
                Path::new("crates/keycord-git/data"),
                Path::new("crates/keycord-shell/data"),
            ],
            "data/resources.xml",
            "compiled.gresource",
        );
    } else {
        glib_build_tools::compile_resources(
            &[&branding_dir, &git_data_dir, &shell_data_dir],
            resources_xml
                .to_str()
                .expect("Resource manifest path must be valid UTF-8"),
            "compiled.gresource",
        );
    }
}

fn write_resources_xml(
    data_dir: &Path,
    resource_id: &str,
    resource_files: &[ResourceFileEntry],
) -> std::path::PathBuf {
    let mut xml = String::from("<gresources>\n");
    writeln!(xml, "\t<gresource prefix=\"{resource_id}\">")
        .expect("Failed to format resource prefix");
    for file in resource_files {
        if let Some(alias) = file.alias.as_deref() {
            writeln!(xml, "\t\t<file alias=\"{alias}\">{}</file>", file.source)
                .expect("Failed to format aliased resource entry");
        } else {
            writeln!(xml, "\t\t<file>{}</file>", file.source)
                .expect("Failed to format resource entry");
        }
    }
    xml.push_str("\t</gresource>\n</gresources>\n");
    let path = data_dir.join("resources.xml");
    write_if_changed(&path, &xml);
    path
}

pub(super) fn write_window_ui(source_root: &Path, out_dir: &Path, gettext_domain: &str) {
    let source = read_composed_window_ui(source_root)
        .unwrap_or_else(|err| panic!("Failed to compose application window UI: {err}"));
    let rendered = with_translation_domain(source, gettext_domain);
    fs::write(out_dir.join("window.ui"), rendered).expect("Failed to write generated window.ui");
}

fn read_composed_window_ui(source_root: &Path) -> Result<String, String> {
    let skeleton_path = source_root.join("crates/keycord-shell/data/window.ui");
    let skeleton = fs::read_to_string(&skeleton_path).map_err(|err| {
        format!(
            "failed to read Shell window skeleton {}: {err}",
            skeleton_path.display()
        )
    })?;
    let fragments = WINDOW_UI_FRAGMENTS
        .iter()
        .map(|(name, relative_path)| {
            let path = source_root.join(relative_path);
            fs::read_to_string(&path)
                .map(|source| (*name, source))
                .map_err(|err| {
                    format!(
                        "failed to read window UI fragment {}: {err}",
                        path.display()
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    compose_window_ui(&skeleton, &fragments)
}

fn compose_window_ui(skeleton: &str, fragments: &[(&str, String)]) -> Result<String, String> {
    let composed = compose_marked_fragments(skeleton, WINDOW_UI_FRAGMENT_NAMESPACE, fragments)?;
    let document = roxmltree::Document::parse(&composed)
        .map_err(|err| format!("composed window UI is not well-formed XML: {err}"))?;
    if !document.root_element().has_tag_name("interface") {
        return Err("composed window UI root must be <interface>".to_string());
    }

    Ok(composed)
}

fn with_translation_domain(source: String, gettext_domain: &str) -> String {
    if source.contains("<interface domain=") {
        return source;
    }

    source.replacen(
        "<interface>",
        &format!("<interface domain=\"{gettext_domain}\">"),
        1,
    )
}

fn collect_icon_assets(dir: &Path, data_dir: &Path, resource_files: &mut Vec<ResourceFileEntry>) {
    for entry in fs::read_dir(dir).expect("Failed to read resource directory") {
        let entry = entry.expect("Failed to read resource directory entry");
        let path = entry.path();

        if path.is_dir() {
            collect_icon_assets(&path, data_dir, resource_files);
        } else if matches!(
            path.extension().and_then(|value| value.to_str()),
            Some("png" | "svg")
        ) && path
            .components()
            .any(|component| component.as_os_str() == "apps")
        {
            let rel = path
                .strip_prefix(data_dir)
                .expect("Resource path should stay within data/");
            resource_files.push(ResourceFileEntry::source(
                rel.to_string_lossy().into_owned(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        collect_icon_assets, compose_window_ui, read_composed_window_ui, with_translation_domain,
        ResourceFileEntry,
    };
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn application_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("Lifecycle crate should live below the application root")
            .to_path_buf()
    }

    fn quoted_values_after<'a>(source: &'a str, prefix: &str) -> Vec<&'a str> {
        let mut values = Vec::new();
        let mut search_start = 0;
        while let Some(offset) = source[search_start..].find(prefix) {
            let value_start = search_start + offset + prefix.len();
            let Some(value_end_offset) = source[value_start..].find('"') else {
                break;
            };
            let value_end = value_start + value_end_offset;
            values.push(&source[value_start..value_end]);
            search_start = value_end + 1;
        }
        values
    }

    fn element_text_values_after<'a>(source: &'a str, opening: &str) -> Vec<&'a str> {
        let mut values = Vec::new();
        let mut search_start = 0;
        while let Some(offset) = source[search_start..].find(opening) {
            let value_start = search_start + offset + opening.len();
            let Some(value_end_offset) = source[value_start..].find('<') else {
                break;
            };
            let value_end = value_start + value_end_offset;
            values.push(source[value_start..value_end].trim());
            search_start = value_end + 1;
        }
        values
    }

    fn translatable_text_values(source: &str) -> Vec<&str> {
        let mut values = Vec::new();
        let mut search_start = 0;
        while let Some(offset) = source[search_start..].find("translatable=\"yes\"") {
            let attribute_start = search_start + offset;
            let Some(text_start_offset) = source[attribute_start..].find('>') else {
                break;
            };
            let text_start = attribute_start + text_start_offset + 1;
            let Some(text_end_offset) = source[text_start..].find('<') else {
                break;
            };
            let text_end = text_start + text_end_offset;
            let value = source[text_start..text_end].trim();
            if !value.is_empty() {
                values.push(value);
            }
            search_start = text_end;
        }
        values
    }

    fn fnv1a(values: &[&str]) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for value in values {
            for byte in value.bytes().chain(std::iter::once(0)) {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        hash
    }

    #[test]
    fn translation_domain_is_injected_once() {
        assert_eq!(
            with_translation_domain("<interface>\n</interface>\n".to_string(), "keycord"),
            "<interface domain=\"keycord\">\n</interface>\n"
        );
        let existing = "<interface domain=\"keycord\">\n</interface>\n";
        assert_eq!(
            with_translation_domain(existing.to_string(), "other"),
            existing
        );
    }

    #[test]
    fn git_subject_icon_keeps_its_existing_resource_path() {
        let source_root = application_root();
        let git_data = source_root.join("crates/keycord-git/data");
        let mut files = Vec::new();
        collect_icon_assets(&git_data, &git_data, &mut files);

        assert_eq!(
            files,
            vec![ResourceFileEntry::source(
                "symbolic/apps/git-symbolic.svg".to_string()
            )]
        );
        assert!(!source_root
            .join("crates/keycord-shell/data/symbolic/apps/git-symbolic.svg")
            .exists());
    }

    #[test]
    fn application_symbolic_icons_are_lifecycle_owned_resources() {
        let source_root = application_root();
        let branding = source_root.join("crates/keycord-lifecycle/data/branding");
        let mut files = Vec::new();
        collect_icon_assets(&branding, &branding, &mut files);
        let sources = files
            .into_iter()
            .map(|entry| entry.source)
            .collect::<BTreeSet<_>>();

        assert!(sources.contains("symbolic/apps/io.github.noobping.keycord-symbolic.svg"));
        assert!(sources.contains("symbolic/apps/io.github.noobping.keycord-beta-symbolic.svg"));
        assert!(!source_root
            .join("crates/keycord-shell/data/symbolic/apps/io.github.noobping.keycord-symbolic.svg")
            .exists());
    }

    #[test]
    fn window_ui_fragments_are_inserted_in_marker_order() {
        let skeleton = "<interface>\n  <!-- keycord-window-fragment:first -->\n  <middle />\n  <!-- keycord-window-fragment:second -->\n</interface>\n";
        let fragments = [
            ("first", "  <first />\n".to_string()),
            ("second", "  <second />\n".to_string()),
        ];

        assert_eq!(
            compose_window_ui(skeleton, &fragments).unwrap(),
            "<interface>\n  <first />\n  <middle />\n  <second />\n</interface>\n"
        );
    }

    #[test]
    fn window_ui_composition_rejects_mismatched_xml_tags() {
        let skeleton = "<interface>\n<!-- keycord-window-fragment:broken -->\n</interface>\n";
        let err = compose_window_ui(skeleton, &[("broken", "<property></child>\n".to_string())])
            .expect_err("mismatched XML tags must fail composition");

        assert!(err.contains("not well-formed XML"));
    }

    #[test]
    fn window_ui_composition_rejects_missing_markers() {
        let err = compose_window_ui("<interface />\n", &[("missing", String::new())])
            .expect_err("missing markers must fail composition");
        assert!(err.contains("missing UI fragment marker `missing`"));
    }

    #[test]
    fn window_ui_composition_rejects_duplicate_markers() {
        let skeleton = "<!-- keycord-window-fragment:duplicate -->\n<!-- keycord-window-fragment:duplicate -->\n";
        let err = compose_window_ui(skeleton, &[("duplicate", String::new())])
            .expect_err("duplicate markers must fail composition");
        assert!(err.contains("duplicate UI fragment marker `duplicate`"));
    }

    #[test]
    fn window_ui_composition_rejects_unresolved_markers() {
        let skeleton =
            "<!-- keycord-window-fragment:known -->\n<!-- keycord-window-fragment:unknown -->\n";
        let err = compose_window_ui(skeleton, &[("known", String::new())])
            .expect_err("unresolved markers must fail composition");
        assert!(err.contains("unresolved UI fragment marker"));
        assert!(err.contains("unknown"));
    }

    #[test]
    fn workspace_window_ui_composes_without_markers() {
        let composed = read_composed_window_ui(&application_root()).unwrap();
        assert!(composed.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(composed.ends_with("</interface>\n"));
        assert!(!composed.contains("keycord-window-fragment:"));
    }

    #[test]
    fn composed_window_inventory_matches_the_pre_split_template() {
        let composed = read_composed_window_ui(&application_root()).unwrap();
        let ids = quoted_values_after(&composed, "id=\"");
        let mut actions = element_text_values_after(&composed, "<property name=\"action-name\">");
        actions.extend(element_text_values_after(
            &composed,
            "<attribute name=\"action\">",
        ));
        let translatable = translatable_text_values(&composed);

        // Captured from the single Shell-owned template immediately before it was split.
        assert_eq!((ids.len(), fnv1a(&ids)), (221, 310_705_445_291_682_508));
        assert_eq!(
            (actions.len(), fnv1a(&actions)),
            (22, 11_318_150_571_725_618_109)
        );
        assert_eq!(
            (translatable.len(), fnv1a(&translatable)),
            (213, 135_236_841_047_613_721)
        );
    }
}
