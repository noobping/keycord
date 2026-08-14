use keycord_ui_fragments::compose_marked_fragments;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const SHORTCUTS_FRAGMENT_NAMESPACE: &str = "keycord-shortcuts-fragment";

const SHORTCUTS_FRAGMENTS: &[(&str, &str)] = &[
    (
        "entries-sections",
        "crates/keycord-entries/data/shortcuts-sections.fragment.ui",
    ),
    (
        "git-list-sync-item",
        "crates/keycord-git/data/shortcuts-list-sync-item.fragment.ui",
    ),
    (
        "stores-navigation-item",
        "crates/keycord-stores/data/shortcuts-navigation-item.fragment.ui",
    ),
    (
        "git-navigation-item",
        "crates/keycord-git/data/shortcuts-navigation-item.fragment.ui",
    ),
    (
        "stores-section",
        "crates/keycord-stores/data/shortcuts-section.fragment.ui",
    ),
    (
        "git-store-items",
        "crates/keycord-git/data/shortcuts-store-items.fragment.ui",
    ),
    (
        "docs-tool-item",
        "crates/keycord-docs/data/shortcuts-tool-item.fragment.ui",
    ),
    (
        "preferences-general-item",
        "crates/keycord-preferences/data/shortcuts-general-item.fragment.ui",
    ),
];

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("Shell crate should live below the application root");
    let skeleton_path = manifest_dir.join("data/shortcuts.ui");
    let skeleton = read_source(&skeleton_path, "Shell shortcuts skeleton");
    let fragments = SHORTCUTS_FRAGMENTS
        .iter()
        .map(|(name, relative_path)| {
            let path = source_root.join(relative_path);
            (*name, read_source(&path, "shortcut UI fragment"))
        })
        .collect::<Vec<_>>();
    let composed = compose_marked_fragments(&skeleton, SHORTCUTS_FRAGMENT_NAMESPACE, &fragments)
        .unwrap_or_else(|err| panic!("Failed to compose application shortcuts UI: {err}"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set for Shell build"));
    fs::write(out_dir.join("shortcuts.ui"), composed)
        .expect("Failed to write generated shortcuts.ui");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", skeleton_path.display());
    for (_, relative_path) in SHORTCUTS_FRAGMENTS {
        println!(
            "cargo:rerun-if-changed={}",
            source_root.join(relative_path).display()
        );
    }
}

fn read_source(path: &Path, description: &str) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {description} {}: {err}", path.display()))
}
