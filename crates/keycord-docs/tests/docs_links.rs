use std::fs;
use std::path::{Path, PathBuf};

fn bundled_markdown_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in
        fs::read_dir(dir).unwrap_or_else(|err| panic!("failed to read {}: {err}", dir.display()))
    {
        let entry = entry.expect("failed to read bundled documentation entry");
        let path = entry.path();
        if path.is_dir() {
            bundled_markdown_files(&path, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
            files.push(path);
        }
    }
}

fn markdown_link_destinations(source: &str) -> Vec<&str> {
    let mut destinations = Vec::new();
    let mut remainder = source;

    while let Some(marker) = remainder.find("](") {
        let after_marker = &remainder[marker + 2..];
        let Some(end) = after_marker.find(')') else {
            break;
        };
        let raw = after_marker[..end].trim();
        let destination = if let Some(raw) = raw.strip_prefix('<') {
            raw.split_once('>').map(|(destination, _)| destination)
        } else {
            raw.split_whitespace().next()
        };
        if let Some(destination) = destination.filter(|destination| !destination.is_empty()) {
            destinations.push(destination);
        }
        remainder = &after_marker[end + 1..];
    }

    destinations
}

fn local_markdown_link_path(destination: &str) -> Option<&str> {
    if destination.starts_with('#')
        || destination.starts_with("http://")
        || destination.starts_with("https://")
        || destination.starts_with("mailto:")
    {
        return None;
    }

    destination
        .split_once('#')
        .map_or(Some(destination), |(path, _)| {
            (!path.is_empty()).then_some(path)
        })
}

#[test]
fn bundled_document_relative_links_resolve() {
    let docs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    let mut documents = Vec::new();
    bundled_markdown_files(&docs_dir, &mut documents);
    documents.sort();
    assert!(!documents.is_empty(), "bundled documentation is missing");

    for document_path in documents {
        let source = fs::read_to_string(&document_path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", document_path.display()));
        let document_dir = document_path
            .parent()
            .expect("bundled document should have a parent directory");

        for destination in markdown_link_destinations(&source) {
            let Some(relative_target) = local_markdown_link_path(destination) else {
                continue;
            };
            let target = document_dir.join(relative_target);
            assert!(
                target.exists(),
                "{} links to missing local target `{destination}` ({})",
                document_path.display(),
                target.display()
            );
        }
    }
}
