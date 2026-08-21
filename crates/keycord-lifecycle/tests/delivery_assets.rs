use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

type CargoArchives = BTreeMap<String, (String, String)>;

fn quoted_lock_field<'a>(package: &'a str, field: &str) -> Option<&'a str> {
    let prefix = format!("{field} = \"");
    package.lines().find_map(|line| {
        line.strip_prefix(&prefix)
            .and_then(|value| value.strip_suffix('"'))
    })
}

fn expected_registry_archives(lock: &str) -> CargoArchives {
    let mut archives = CargoArchives::new();

    for package in lock.split("[[package]]").skip(1) {
        let Some(source) = quoted_lock_field(package, "source") else {
            continue;
        };
        if !source.starts_with("registry+") {
            continue;
        }

        let name = quoted_lock_field(package, "name").expect("registry package should have a name");
        let version =
            quoted_lock_field(package, "version").expect("registry package should have a version");
        let checksum = quoted_lock_field(package, "checksum")
            .expect("registry package should have a checksum");
        let destination = format!("cargo/vendor/{name}-{version}");
        let url = format!("https://static.crates.io/crates/{name}/{name}-{version}.crate");

        assert!(
            archives
                .insert(destination.clone(), (url, checksum.to_string()))
                .is_none(),
            "duplicate registry package destination in Cargo.lock: {destination}"
        );
    }

    archives
}

fn yaml_field<'a>(record: &'a str, field: &str) -> Option<&'a str> {
    let prefix = format!("{field}: ");
    record
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
}

fn actual_registry_archives(sources: &str) -> CargoArchives {
    let mut archives = CargoArchives::new();

    for record in sources.split("- type: ").skip(1) {
        if record.lines().next().map(str::trim) != Some("archive") {
            continue;
        }

        let url = yaml_field(record, "url").expect("archive source should have a URL");
        let checksum = yaml_field(record, "sha256").expect("archive source should have a checksum");
        let destination =
            yaml_field(record, "dest").expect("archive source should have a destination");
        assert!(
            archives
                .insert(
                    destination.to_string(),
                    (url.to_string(), checksum.to_string()),
                )
                .is_none(),
            "duplicate archive destination in cargo-sources.yml: {destination}"
        );
    }

    archives
}

#[test]
fn meson_renders_tracked_lifecycle_metadata_templates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Lifecycle crate should live below the application root");
    let meson = fs::read_to_string(root.join("meson.build")).expect("read root meson.build");
    let templates = [
        "crates/keycord-lifecycle/data/keycord.desktop.in",
        "crates/keycord-lifecycle/data/keycord-search-provider.ini.in",
        "crates/keycord-lifecycle/data/keycord-search-provider.service.in",
    ];

    for relative_path in templates {
        assert!(
            root.join(relative_path).is_file(),
            "missing {relative_path}"
        );
        assert!(
            meson.contains(&format!("input: '{relative_path}'")),
            "Meson must render tracked Lifecycle template {relative_path}"
        );
    }

    assert!(!meson.contains("'crates/keycord-lifecycle/data/keycord.desktop',"));
    assert!(!meson.contains("'crates/keycord-lifecycle/data/keycord-search-provider.ini',"));
    assert!(!meson.contains("'crates/keycord-lifecycle/data/keycord-search-provider.service',"));
}

#[test]
fn meson_installs_localized_application_metadata_and_catalogs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Lifecycle crate should live below the application root");
    let meson = fs::read_to_string(root.join("meson.build")).expect("read root meson.build");
    let po_meson = fs::read_to_string(root.join("po/meson.build")).expect("read po/meson.build");
    let linguas = fs::read_to_string(root.join("po/LINGUAS")).expect("read po/LINGUAS");

    assert!(meson.contains("i18n = import('i18n')"));
    assert!(meson.contains("subdir('po')"));
    assert!(meson.contains("type: 'desktop'"));
    assert!(meson.contains("output: 'io.github.noobping.keycord.desktop'"));
    assert!(meson.contains("type: 'xml'"));
    assert!(meson.contains("output: 'io.github.noobping.keycord.metainfo.xml'"));
    assert!(po_meson.contains("i18n.gettext('keycord'"));
    assert_eq!(
        linguas.split_whitespace().collect::<Vec<_>>(),
        ["de", "en", "es", "fr", "it", "ja", "nl", "pl", "pt", "pt_BR", "sv", "zh_CN",]
    );
}

#[test]
fn flatpak_build_consumes_the_locked_offline_cargo_sources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Lifecycle crate should live below the application root");
    let manifest = fs::read_to_string(root.join("io.github.noobping.keycord.json"))
        .expect("read Flatpak manifest");
    let meson = fs::read_to_string(root.join("meson.build")).expect("read root meson.build");

    assert!(manifest.contains("\"cargo-sources.yml\""));
    assert!(!manifest.contains("--share=network"));
    assert!(manifest.contains("\"CARGO_NET_OFFLINE\": \"true\""));
    assert!(meson.contains("\"--locked\""));
    assert!(!meson.contains("\"--offline\""));
}

#[test]
fn flatpak_cargo_archives_match_the_lockfile_exactly() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("Lifecycle crate should live below the application root");
    let lock = fs::read_to_string(root.join("Cargo.lock")).expect("read Cargo.lock");
    let sources =
        fs::read_to_string(root.join("cargo-sources.yml")).expect("read cargo-sources.yml");

    let expected = expected_registry_archives(&lock);
    let actual = actual_registry_archives(&sources);
    let missing = expected
        .keys()
        .filter(|destination| !actual.contains_key(*destination))
        .cloned()
        .collect::<Vec<_>>();
    let stale = actual
        .keys()
        .filter(|destination| !expected.contains_key(*destination))
        .cloned()
        .collect::<Vec<_>>();

    assert!(missing.is_empty(), "missing Cargo archives: {missing:?}");
    assert!(stale.is_empty(), "stale Cargo archives: {stale:?}");
    for (destination, expected_source) in expected {
        assert_eq!(
            actual.get(&destination),
            Some(&expected_source),
            "archive URL/checksum mismatch for {destination}"
        );
    }
}
