use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CompiledDocSource {
    canonical_path: String,
    locale: Option<String>,
    relative_path: String,
}

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set for build script"));
    write_docs_manifest(Path::new("docs"), &out_dir);

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=docs");
}

fn write_docs_manifest(docs_dir: &Path, out_dir: &Path) {
    let mut sources = collect_doc_sources(docs_dir);
    sources.sort();

    let mut output = String::from("const DOC_SOURCES: &[CompiledDocumentSource] = &[\n");
    for source in sources {
        let locale = match source.locale {
            Some(locale) => format!("Some({locale:?})"),
            None => "None".to_string(),
        };
        writeln!(
            output,
            "    CompiledDocumentSource {{ path: {:?}, locale: {}, source: include_str!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/{}\")) }},",
            source.canonical_path,
            locale,
            source.relative_path
        )
        .expect("Failed to format docs manifest entry");
    }
    output.push_str("];\n");

    fs::write(out_dir.join("docs_manifest.rs"), output)
        .expect("Failed to write documentation source manifest");
}

fn collect_doc_sources(docs_dir: &Path) -> Vec<CompiledDocSource> {
    let mut sources = Vec::new();

    for entry in fs::read_dir(docs_dir)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", docs_dir.display()))
    {
        let entry = entry.expect("Failed to read docs directory entry");
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let Some((canonical_path, locale)) = parse_doc_source_file_name(file_name) else {
            continue;
        };

        sources.push(CompiledDocSource {
            canonical_path,
            locale,
            relative_path: format!("docs/{file_name}"),
        });
    }

    sources
}

fn parse_doc_source_file_name(file_name: &str) -> Option<(String, Option<String>)> {
    let stem = file_name.strip_suffix(".md")?;

    if let Some((base, locale)) = stem.rsplit_once('.') {
        if looks_like_locale_tag(locale) {
            return Some((format!("{base}.md"), Some(locale.to_string())));
        }
    }

    Some((file_name.to_string(), None))
}

fn looks_like_locale_tag(value: &str) -> bool {
    value.len() >= 2
        && value.chars().any(|ch| ch.is_ascii_alphabetic())
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}

#[cfg(test)]
mod tests {
    use super::parse_doc_source_file_name;

    #[test]
    fn identifies_default_and_localized_markdown_sources() {
        assert_eq!(
            parse_doc_source_file_name("search.md"),
            Some(("search.md".to_string(), None))
        );
        assert_eq!(
            parse_doc_source_file_name("search.nl.md"),
            Some(("search.md".to_string(), Some("nl".to_string())))
        );
        assert_eq!(parse_doc_source_file_name("image.svg"), None);
    }
}
