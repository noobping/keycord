//! Translation source extraction and PO/MO generation.
use super::write_if_changed;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

type Catalog = BTreeMap<String, CatalogEntry>;

const NON_APPLICATION_RUST_SOURCE_DIRS: &[&str] = &[
    "crates/keycord-architecture",
    "crates/keycord-ui-fragments",
    "crates/keycord-lifecycle/src/build_support",
];
const NON_APPLICATION_RUST_DIRECTORY_NAMES: &[&str] = &["benches", "examples", "tests"];

#[derive(Default)]
struct CatalogEntry {
    references: BTreeSet<String>,
}

#[derive(Clone, Debug, Default)]
struct PoEntry {
    msgid: String,
    msgid_plural: Option<String>,
    msgstr: String,
    msgstr_plural: BTreeMap<usize, String>,
}

#[derive(Clone, Copy, Debug)]
enum ActivePoField {
    Id,
    IdPlural,
    Str,
    StrPlural(usize),
}

pub(super) fn build_catalogs(
    source_root: &Path,
    out_dir: &Path,
    gettext_domain: &str,
    package_name: &str,
    package_version: &str,
    desktop_entry: &str,
) -> Vec<String> {
    let po_dir = source_root.join("po");
    let locale_dir = out_dir.join("locale");
    fs::create_dir_all(&po_dir).expect("Failed to create po directory");

    let mut catalog = Catalog::new();
    // Root data contains generated copies; canonical translatable UI lives with its owning crate.
    collect_ui_strings_from_dir(source_root, &source_root.join("crates"), &mut catalog);
    collect_metainfo_strings(
        source_root,
        &source_root.join("crates/keycord-lifecycle/data/metainfo.xml"),
        &mut catalog,
    );
    collect_desktop_strings(
        source_root,
        &source_root.join("crates/keycord-lifecycle/data/keycord.desktop.in"),
        desktop_entry,
        &mut catalog,
    );
    collect_rust_strings(source_root, &source_root.join("src"), &mut catalog);
    collect_rust_strings(source_root, &source_root.join("crates"), &mut catalog);

    let pot_path = po_dir.join(format!("{gettext_domain}.pot"));
    let en_path = po_dir.join("en.po");
    write_if_changed(
        &pot_path,
        render_pot_catalog(&catalog, package_name, package_version),
    );
    write_if_changed(
        &en_path,
        render_po_catalog(&catalog, "en", package_name, package_version),
    );

    compile_translations(&po_dir, &locale_dir, gettext_domain)
}

fn collect_ui_strings_from_dir(source_root: &Path, dir: &Path, catalog: &mut Catalog) {
    for entry in
        fs::read_dir(dir).unwrap_or_else(|err| panic!("Failed to read {}: {err}", dir.display()))
    {
        let entry = entry.expect("Failed to read UI source directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_ui_strings_from_dir(source_root, &path, catalog);
        } else if path.extension().and_then(|value| value.to_str()) == Some("ui") {
            collect_ui_strings(source_root, &path, catalog);
        }
    }
}

fn collect_ui_strings(source_root: &Path, path: &Path, catalog: &mut Catalog) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    let mut search_start = 0usize;

    while let Some(relative_index) = source[search_start..].find("translatable=\"yes\"") {
        let attr_index = search_start + relative_index;
        let text_start = source[attr_index..]
            .find('>')
            .map(|offset| attr_index + offset + 1);
        let Some(text_start) = text_start else {
            break;
        };
        let text_end = source[text_start..]
            .find('<')
            .map(|offset| text_start + offset);
        let Some(text_end) = text_end else {
            break;
        };

        let text = decode_xml_entities(source[text_start..text_end].trim());
        if !text.is_empty() {
            add_catalog_message(
                catalog,
                &text,
                source_reference(source_root, path, line_number(&source, attr_index)),
            );
        }

        search_start = text_end;
    }
}

fn collect_metainfo_strings(source_root: &Path, path: &Path, catalog: &mut Catalog) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    let bytes = source.as_bytes();
    let mut stack = Vec::new();
    let mut index = 0usize;
    let mut text_start = None;

    while index < bytes.len() {
        if bytes[index] != b'<' {
            text_start.get_or_insert(index);
            index += 1;
            continue;
        }

        if let Some(start) = text_start.take() {
            add_metainfo_text(source_root, catalog, &source, path, &stack, start, index);
        }

        if source[index..].starts_with("<!--") {
            index = source[index + 4..]
                .find("-->")
                .map(|offset| index + 4 + offset + 3)
                .unwrap_or(bytes.len());
            continue;
        }

        if source[index..].starts_with("<![CDATA[") {
            let data_start = index + 9;
            let data_end = source[data_start..]
                .find("]]>")
                .map(|offset| data_start + offset)
                .unwrap_or(bytes.len());
            add_metainfo_text(
                source_root,
                catalog,
                &source,
                path,
                &stack,
                data_start,
                data_end,
            );
            index = data_end.saturating_add(3).min(bytes.len());
            continue;
        }

        if source[index..].starts_with("<?") {
            index = source[index + 2..]
                .find("?>")
                .map(|offset| index + 2 + offset + 2)
                .unwrap_or(bytes.len());
            continue;
        }

        let tag_end = find_xml_tag_end(bytes, index).unwrap_or(bytes.len().saturating_sub(1));
        let tag = &source[index + 1..tag_end];
        let trimmed = tag.trim();

        if trimmed.starts_with('/') {
            stack.pop();
            index = tag_end + 1;
            continue;
        }

        let tag_name = xml_tag_name(trimmed);
        let inherited_skip = stack.last().copied().unwrap_or(false);
        let skip =
            inherited_skip || tag_has_translate_no(trimmed) || matches!(tag_name, "translation");
        let self_closing = trimmed.ends_with('/');

        if !self_closing {
            stack.push(skip);
        }

        index = tag_end + 1;
    }

    if let Some(start) = text_start {
        add_metainfo_text(
            source_root,
            catalog,
            &source,
            path,
            &stack,
            start,
            bytes.len(),
        );
    }
}

fn collect_desktop_strings(source_root: &Path, path: &Path, source: &str, catalog: &mut Catalog) {
    for (line_index, raw_line) in source.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        if key.contains('[') {
            continue;
        }

        if !matches!(key, "Name" | "GenericName" | "Comment" | "Keywords") {
            continue;
        }

        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        add_catalog_message(
            catalog,
            value,
            source_reference(source_root, path, line_index + 1),
        );
    }
}

pub(super) fn merge_desktop_translations(source_root: &Path, source: &str) -> String {
    let po_dir = source_root.join("po");
    let translations = discover_po_locales(&po_dir)
        .into_iter()
        .map(|locale| {
            let entries = parse_po_entries(
                &fs::read_to_string(po_dir.join(format!("{locale}.po")))
                    .unwrap_or_else(|err| panic!("Failed to read {locale} translation: {err}")),
            );
            let messages = entries
                .into_iter()
                .filter(|entry| !entry.msgid.is_empty() && !entry.msgstr.is_empty())
                .map(|entry| (entry.msgid, entry.msgstr))
                .collect::<BTreeMap<_, _>>();
            (locale, messages)
        })
        .collect::<Vec<_>>();

    let mut output = String::new();
    for raw_line in source.lines() {
        writeln!(output, "{raw_line}").expect("Failed to render desktop entry");

        let line = raw_line.trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.contains('[')
            || !matches!(key, "Name" | "GenericName" | "Comment" | "Keywords")
            || value.is_empty()
        {
            continue;
        }

        for (locale, messages) in &translations {
            let Some(translation) = messages.get(value) else {
                continue;
            };
            let translation = escape_desktop_value(translation);
            writeln!(output, "{key}[{locale}]={translation}")
                .expect("Failed to render localized desktop entry");
        }
    }

    output
}

fn escape_desktop_value(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn add_metainfo_text(
    source_root: &Path,
    catalog: &mut Catalog,
    source: &str,
    path: &Path,
    stack: &[bool],
    start: usize,
    end: usize,
) {
    if stack.last().copied().unwrap_or(false) {
        return;
    }

    let text = normalize_xml_text(&decode_xml_entities(&source[start..end]));
    if text.is_empty() {
        return;
    }

    add_catalog_message(
        catalog,
        &text,
        source_reference(source_root, path, line_number(source, start)),
    );
}

fn normalize_xml_text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_xml_tag_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = start + 1;
    let mut quote = None;

    while index < bytes.len() {
        match bytes[index] {
            b'\'' | b'"' if quote.is_none() => quote = Some(bytes[index]),
            byte if Some(byte) == quote => quote = None,
            b'>' if quote.is_none() => return Some(index),
            _ => {}
        }
        index += 1;
    }

    None
}

fn xml_tag_name(tag: &str) -> &str {
    let trimmed = tag.trim_start();
    let start = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let end = start
        .find(|ch: char| ch.is_whitespace() || ch == '/')
        .unwrap_or(start.len());
    &start[..end]
}

fn tag_has_translate_no(tag: &str) -> bool {
    tag.contains("translate=\"no\"") || tag.contains("translate='no'")
}

fn collect_rust_strings(source_root: &Path, dir: &Path, catalog: &mut Catalog) {
    for entry in
        fs::read_dir(dir).unwrap_or_else(|err| panic!("Failed to read {}: {err}", dir.display()))
    {
        let entry = entry.expect("Failed to read source directory entry");
        let path = entry.path();

        if path.is_dir() {
            if non_application_rust_source_dir(source_root, &path) {
                continue;
            }
            collect_rust_strings(source_root, &path, catalog);
            continue;
        }

        if path.extension().and_then(|value| value.to_str()) != Some("rs") {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if file_name == "build.rs" || file_name.contains("test") {
            continue;
        }

        collect_rust_strings_from_file(source_root, &path, catalog);
    }
}

fn non_application_rust_source_dir(source_root: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(source_root).unwrap_or(path);
    NON_APPLICATION_RUST_SOURCE_DIRS
        .iter()
        .any(|excluded| relative.starts_with(excluded))
        || relative.components().any(|component| {
            let component = component.as_os_str().to_string_lossy();
            NON_APPLICATION_RUST_DIRECTORY_NAMES.contains(&component.as_ref())
        })
}

fn collect_rust_strings_from_file(source_root: &Path, path: &Path, catalog: &mut Catalog) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    collect_rust_strings_from_source(source_root, path, &source, catalog);
}

fn collect_rust_strings_from_source(
    source_root: &Path,
    path: &Path,
    source: &str,
    catalog: &mut Catalog,
) {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut line = 1usize;

    while index < bytes.len() {
        if bytes[index] == b'\n' {
            line += 1;
            index += 1;
            continue;
        }

        if skip_raw_string(bytes, &mut index, &mut line) {
            continue;
        }

        if skip_cfg_test_item(bytes, &mut index, &mut line) {
            continue;
        }

        if skip_test_item(bytes, &mut index, &mut line) {
            continue;
        }

        if skip_non_error_rust_attribute(bytes, &mut index, &mut line) {
            continue;
        }

        if skip_rust_comment(bytes, &mut index, &mut line) {
            continue;
        }

        if bytes[index] == b'\'' && looks_like_char_literal(bytes, index) {
            index += 1;
            while index < bytes.len() {
                match bytes[index] {
                    b'\\' => index += 2,
                    b'\'' => {
                        index += 1;
                        break;
                    }
                    b'\n' => {
                        line += 1;
                        index += 1;
                    }
                    _ => index += 1,
                }
            }
            continue;
        }

        if bytes[index] != b'"' {
            index += 1;
            continue;
        }

        let literal_start = index;
        let literal_line = line;
        index += 1;
        let mut value = String::new();

        while index < bytes.len() {
            match bytes[index] {
                b'\\' => {
                    index += 1;
                    if index >= bytes.len() {
                        break;
                    }
                    push_unescaped_rust_char(bytes, &mut index, &mut value);
                }
                b'"' => {
                    index += 1;
                    break;
                }
                b'\n' => {
                    line += 1;
                    value.push('\n');
                    index += 1;
                }
                byte if byte.is_ascii() => {
                    value.push(byte as char);
                    index += 1;
                }
                _ => {
                    let character = source[index..]
                        .chars()
                        .next()
                        .expect("Rust source should remain valid UTF-8");
                    value.push(character);
                    index += character.len_utf8();
                }
            }
        }

        if looks_translatable_rust_string(&value)
            && (rust_literal_is_explicitly_translatable(source, literal_start)
                || !rust_literal_has_non_translatable_context(source, literal_start))
        {
            add_catalog_message(
                catalog,
                value.trim(),
                source_reference(source_root, path, literal_line),
            );
        }
    }
}

fn rust_literal_is_explicitly_translatable(source: &str, literal_start: usize) -> bool {
    ["gettext", "ngettext", "pgettext"]
        .iter()
        .any(|function| rust_literal_is_inside_call(source, literal_start, function))
}

fn rust_literal_has_non_translatable_context(source: &str, literal_start: usize) -> bool {
    let prefix = source[..literal_start].trim_end();
    const IMMEDIATE_PREFIXES: &[&str] = &[
        ".contains(",
        ".ends_with(",
        ".expect(",
        ".expect_err(",
        ".starts_with(",
        ".strip_prefix(",
        ".strip_suffix(",
        "assert!(",
        "assert_eq!(",
        "assert_ne!(",
        "debug_assert!(",
        "debug_assert_eq!(",
        "debug_assert_ne!(",
        "panic!(",
        "unreachable!(",
    ];

    IMMEDIATE_PREFIXES
        .iter()
        .any(|candidate| prefix.ends_with(candidate))
        || [
            "eprintln",
            "log_error",
            "log_info",
            "log_warning",
            "message_contains_any",
            "println",
        ]
        .iter()
        .any(|function| rust_literal_is_inside_call(source, literal_start, function))
}

fn rust_literal_is_inside_call(source: &str, literal_start: usize, function: &str) -> bool {
    let prefix = &source[..literal_start];
    [format!("{function}("), format!("{function}!(")]
        .iter()
        .any(|needle| {
            prefix
                .match_indices(needle)
                .any(|(call_start, _)| rust_call_is_open(&prefix[call_start + needle.len()..]))
        })
}

fn rust_call_is_open(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut depth = 1usize;
    let mut line = 0usize;

    while index < bytes.len() {
        if skip_raw_string(bytes, &mut index, &mut line)
            || skip_rust_comment(bytes, &mut index, &mut line)
            || skip_rust_quoted_literal(bytes, &mut index, &mut line)
        {
            continue;
        }

        match bytes[index] {
            b'(' => depth += 1,
            b')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return false;
                }
            }
            _ => {}
        }
        index += 1;
    }

    true
}

fn push_unescaped_rust_char(bytes: &[u8], index: &mut usize, value: &mut String) {
    match bytes[*index] {
        b'\\' => {
            value.push('\\');
            *index += 1;
        }
        b'"' => {
            value.push('"');
            *index += 1;
        }
        b'n' => {
            value.push('\n');
            *index += 1;
        }
        b'r' => {
            value.push('\r');
            *index += 1;
        }
        b't' => {
            value.push('\t');
            *index += 1;
        }
        b'0' => {
            value.push('\0');
            *index += 1;
        }
        b'u' if bytes.get(*index + 1) == Some(&b'{') => {
            *index += 2;
            let start = *index;
            while *index < bytes.len() && bytes[*index] != b'}' {
                *index += 1;
            }
            let escape = std::str::from_utf8(&bytes[start..*index]).unwrap_or_default();
            if let Ok(codepoint) = u32::from_str_radix(escape, 16) {
                if let Some(ch) = char::from_u32(codepoint) {
                    value.push(ch);
                }
            }
            if *index < bytes.len() {
                *index += 1;
            }
        }
        other => {
            value.push(other as char);
            *index += 1;
        }
    }
}

fn looks_translatable_rust_string(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }

    if looks_like_non_translatable_rust_syntax(trimmed) {
        return false;
    }

    if trimmed.ends_with(".md") && !trimmed.chars().any(char::is_whitespace) {
        return false;
    }

    if trimmed.starts_with("[Desktop Entry]")
        || trimmed.starts_with("[Shell Search Provider]")
        || trimmed.starts_with("[D-BUS Service]")
    {
        return false;
    }

    if trimmed.starts_with('#') || trimmed.chars().any(|ch| ch == '\u{1b}') {
        return false;
    }

    if !trimmed.chars().any(char::is_whitespace) && trimmed.contains('=') {
        return false;
    }

    if trimmed.starts_with('/') || trimmed.starts_with("./") {
        return false;
    }

    if !trimmed.chars().any(char::is_whitespace)
        && (trimmed.starts_with('.')
            || trimmed.starts_with("../")
            || trimmed.contains('/')
            || trimmed.contains('@'))
    {
        return false;
    }

    if trimmed.contains("example.com") {
        return false;
    }

    if trimmed.starts_with("io.github.")
        || trimmed.starts_with("org.")
        || trimmed.starts_with("app.")
        || trimmed.starts_with("win.")
        || trimmed.starts_with("edit-")
        || trimmed.starts_with("document-")
        || trimmed.starts_with("folder-")
        || trimmed.starts_with("go-")
        || trimmed.starts_with("list-")
        || trimmed.starts_with("open-")
        || trimmed.starts_with("view-")
    {
        return false;
    }

    if trimmed.contains("::") {
        return false;
    }

    if !trimmed.chars().any(char::is_alphabetic) {
        return false;
    }

    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-' | '.'))
    {
        return false;
    }

    if trimmed.chars().any(char::is_whitespace) {
        return true;
    }

    if trimmed
        .chars()
        .any(|ch| matches!(ch, '.' | '!' | '?' | ':'))
    {
        return true;
    }

    trimmed
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_uppercase())
        && trimmed.chars().any(|ch| ch.is_ascii_lowercase())
}

fn looks_like_non_translatable_rust_syntax(value: &str) -> bool {
    looks_like_ascii_armor_boundary(value)
        || value.starts_with("--")
        || value.starts_with("*.")
        || value.starts_with("Exec=")
        || value.starts_with("flatpak override ")
        || (value.starts_with("git ") && value != "git command")
        || value.starts_with("GIT_{")
        || value.starts_with("HEAD^")
        || looks_like_markup_template(value)
        || value.starts_with("@define-color ")
        || value.starts_with("D:P(")
        || matches!(
            value,
            "passkey: [redacted]" | "stdin: [redacted]" | "stdin: provided"
        )
        || value.starts_with("signal {")
        || value.starts_with("stream logger panicked while reading ")
        || value.contains("\n$ ")
        || value == "find \"{}\" is \"{}\""
        || value.starts_with("{REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER}\n")
        || looks_like_constant_placeholder_record(value)
        || looks_like_technical_identifier(value)
        || looks_like_field_template(value)
}

fn looks_like_markup_template(value: &str) -> bool {
    if !value.starts_with('<') {
        return false;
    }

    let mut inside_tag = false;
    let mut inside_placeholder = false;
    let mut saw_tag = false;
    for ch in value.chars() {
        match ch {
            '<' if !inside_placeholder => {
                inside_tag = true;
                saw_tag = true;
            }
            '>' if inside_tag => inside_tag = false,
            '{' if !inside_tag => inside_placeholder = true,
            '}' if inside_placeholder => inside_placeholder = false,
            _ if !inside_tag && !inside_placeholder && ch.is_alphabetic() => return false,
            _ => {}
        }
    }

    saw_tag
}

fn looks_like_constant_placeholder_record(value: &str) -> bool {
    let Some((field, stored_value)) = value.split_once(": ") else {
        return false;
    };
    let Some(field) = field
        .strip_prefix('{')
        .and_then(|field| field.strip_suffix('}'))
    else {
        return false;
    };

    !field.is_empty()
        && field
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
        && stored_value.starts_with('{')
        && stored_value.ends_with('}')
}

fn looks_like_ascii_armor_boundary(value: &str) -> bool {
    let Some(body) = value
        .strip_prefix("-----BEGIN ")
        .or_else(|| value.strip_prefix("-----END "))
    else {
        return false;
    };

    body.ends_with("-----")
}

fn looks_like_technical_identifier(value: &str) -> bool {
    if value.chars().any(char::is_whitespace) {
        return false;
    }

    if value.starts_with('{') && value.contains('}') {
        return true;
    }

    if value.starts_with("update-{") && value.ends_with('}') {
        return true;
    }

    if value.starts_with("X-") {
        return true;
    }

    if value
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
        && (value.contains('.') || value.ends_with(':'))
    {
        return true;
    }

    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        && value.chars().skip(1).any(|ch| ch.is_ascii_uppercase())
}

fn looks_like_field_template(value: &str) -> bool {
    let mut lines = value.lines();
    let Some(first) = lines.next() else {
        return false;
    };
    let rest = lines.collect::<Vec<_>>();
    !rest.is_empty()
        && std::iter::once(first)
            .chain(rest)
            .all(|line| field_template_name(line.trim()).is_some())
}

fn field_template_name(line: &str) -> Option<&str> {
    let name = line.strip_suffix(':')?;
    (!name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')))
    .then_some(name)
}

fn looks_like_char_literal(bytes: &[u8], index: usize) -> bool {
    let mut cursor = index + 1;
    let Some(first) = bytes.get(cursor).copied() else {
        return false;
    };

    if first == b'\\' {
        cursor += 1;
        match bytes.get(cursor).copied() {
            Some(b'u') if bytes.get(cursor + 1) == Some(&b'{') => {
                cursor += 2;
                while cursor < bytes.len() && bytes[cursor] != b'}' {
                    cursor += 1;
                }
                cursor += usize::from(cursor < bytes.len());
            }
            Some(b'x') => cursor = (cursor + 3).min(bytes.len()),
            Some(_) => cursor += 1,
            None => return false,
        }
    } else {
        let width = match first {
            0x00..=0x7f => 1,
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => return false,
        };
        cursor = (cursor + width).min(bytes.len());
    }

    bytes.get(cursor) == Some(&b'\'')
}

fn skip_raw_string(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    if bytes[*index] != b'r' {
        return false;
    }

    let mut cursor = *index + 1;
    let mut hashes = 0usize;
    while cursor < bytes.len() && bytes[cursor] == b'#' {
        hashes += 1;
        cursor += 1;
    }

    if cursor >= bytes.len() || bytes[cursor] != b'"' {
        return false;
    }

    *index = cursor + 1;
    while *index < bytes.len() {
        if bytes[*index] == b'\n' {
            *line += 1;
            *index += 1;
            continue;
        }

        if bytes[*index] == b'"'
            && bytes
                .get(*index + 1..*index + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            *index += 1 + hashes;
            return true;
        }

        *index += 1;
    }

    true
}

fn skip_cfg_test_item(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    let Some(attribute_end) = find_rust_attribute_end(bytes, *index) else {
        return false;
    };
    let attribute = std::str::from_utf8(&bytes[*index + 2..attribute_end - 1])
        .unwrap_or_default()
        .trim();
    if !cfg_attribute_requires_test(attribute) {
        return false;
    }

    advance_rust_cursor(bytes, index, line, attribute_end);
    skip_following_rust_item(bytes, index, line);
    true
}

fn skip_test_item(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    let Some(attribute_end) = find_rust_attribute_end(bytes, *index) else {
        return false;
    };
    let attribute = std::str::from_utf8(&bytes[*index + 2..attribute_end - 1]).unwrap_or_default();
    if rust_attribute_name(attribute) != Some("test") {
        return false;
    }

    advance_rust_cursor(bytes, index, line, attribute_end);
    skip_following_rust_item(bytes, index, line);
    true
}

fn skip_following_rust_item(bytes: &[u8], index: &mut usize, line: &mut usize) {
    while *index < bytes.len() {
        if skip_raw_string(bytes, index, line)
            || skip_rust_comment(bytes, index, line)
            || skip_rust_quoted_literal(bytes, index, line)
        {
            continue;
        }

        match bytes[*index] {
            b'{' => {
                *index += 1;
                skip_rust_block(bytes, index, line);
                return;
            }
            b';' => {
                *index += 1;
                return;
            }
            b'\n' => {
                *line += 1;
                *index += 1;
            }
            _ => *index += 1,
        }
    }
}

fn skip_rust_block(bytes: &[u8], index: &mut usize, line: &mut usize) {
    let mut depth = 1usize;
    while *index < bytes.len() && depth > 0 {
        if skip_raw_string(bytes, index, line)
            || skip_rust_comment(bytes, index, line)
            || skip_rust_quoted_literal(bytes, index, line)
        {
            continue;
        }

        match bytes[*index] {
            b'{' => {
                depth += 1;
                *index += 1;
            }
            b'}' => {
                depth -= 1;
                *index += 1;
            }
            b'\n' => {
                *line += 1;
                *index += 1;
            }
            _ => *index += 1,
        }
    }
}

fn skip_rust_comment(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    if bytes.get(*index..*index + 2) == Some(b"//") {
        *index += 2;
        while *index < bytes.len() {
            if bytes[*index] == b'\n' {
                *line += 1;
                *index += 1;
                break;
            }
            *index += 1;
        }
        return true;
    }

    if bytes.get(*index..*index + 2) != Some(b"/*") {
        return false;
    }

    *index += 2;
    let mut depth = 1usize;
    while *index < bytes.len() && depth > 0 {
        if bytes.get(*index..*index + 2) == Some(b"/*") {
            depth += 1;
            *index += 2;
        } else if bytes.get(*index..*index + 2) == Some(b"*/") {
            depth -= 1;
            *index += 2;
        } else {
            if bytes[*index] == b'\n' {
                *line += 1;
            }
            *index += 1;
        }
    }

    true
}

fn skip_rust_quoted_literal(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    let delimiter = match bytes.get(*index) {
        Some(b'"') => b'"',
        Some(b'\'') if looks_like_char_literal(bytes, *index) => b'\'',
        _ => return false,
    };

    *index += 1;
    while *index < bytes.len() {
        match bytes[*index] {
            b'\\' => {
                *index += 1;
                if bytes.get(*index) == Some(&b'\n') {
                    *line += 1;
                }
                *index = (*index + 1).min(bytes.len());
            }
            byte if byte == delimiter => {
                *index += 1;
                break;
            }
            b'\n' => {
                *line += 1;
                *index += 1;
            }
            _ => *index += 1,
        }
    }

    true
}

fn find_rust_attribute_end(bytes: &[u8], start: usize) -> Option<usize> {
    if bytes.get(start..start + 2) != Some(b"#[") {
        return None;
    }

    let mut index = start + 2;
    let mut depth = 1usize;
    let mut ignored_line = 0usize;
    while index < bytes.len() {
        if skip_raw_string(bytes, &mut index, &mut ignored_line)
            || skip_rust_comment(bytes, &mut index, &mut ignored_line)
            || skip_rust_quoted_literal(bytes, &mut index, &mut ignored_line)
        {
            continue;
        }

        match bytes[index] {
            b'[' => {
                depth += 1;
                index += 1;
            }
            b']' => {
                depth -= 1;
                index += 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => index += 1,
        }
    }

    None
}

fn rust_attribute_name(attribute: &str) -> Option<&str> {
    let path = attribute
        .trim_start()
        .split(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | ':')))
        .next()?;
    (!path.is_empty()).then(|| path.rsplit("::").next().unwrap_or(path))
}

fn cfg_attribute_requires_test(attribute: &str) -> bool {
    if rust_attribute_name(attribute) != Some("cfg") {
        return false;
    }

    let Some(arguments) = attribute
        .trim_start()
        .strip_prefix("cfg")
        .map(str::trim_start)
        .and_then(|arguments| arguments.strip_prefix('('))
        .and_then(|arguments| arguments.strip_suffix(')'))
    else {
        return false;
    };
    cfg_expression_requires_test(arguments)
}

fn cfg_expression_requires_test(expression: &str) -> bool {
    let expression = expression.trim();
    if expression == "test" {
        return true;
    }

    let Some(arguments) = expression
        .strip_prefix("all")
        .map(str::trim_start)
        .and_then(|arguments| arguments.strip_prefix('('))
        .and_then(|arguments| arguments.strip_suffix(')'))
    else {
        return false;
    };

    top_level_cfg_arguments(arguments)
        .into_iter()
        .any(cfg_expression_requires_test)
}

fn top_level_cfg_arguments(arguments: &str) -> Vec<&str> {
    let bytes = arguments.as_bytes();
    let mut result = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut quote = false;
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\\' if quote => index = (index + 2).min(bytes.len()),
            b'"' => {
                quote = !quote;
                index += 1;
            }
            b'(' if !quote => {
                depth += 1;
                index += 1;
            }
            b')' if !quote => {
                depth = depth.saturating_sub(1);
                index += 1;
            }
            b',' if !quote && depth == 0 => {
                result.push(arguments[start..index].trim());
                index += 1;
                start = index;
            }
            _ => index += 1,
        }
    }
    result.push(arguments[start..].trim());
    result
}

fn advance_rust_cursor(bytes: &[u8], index: &mut usize, line: &mut usize, end: usize) {
    *line += bytes[*index..end]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count();
    *index = end;
}

fn skip_non_error_rust_attribute(bytes: &[u8], index: &mut usize, line: &mut usize) -> bool {
    let Some(attribute_end) = find_rust_attribute_end(bytes, *index) else {
        return false;
    };
    let attribute = std::str::from_utf8(&bytes[*index + 2..attribute_end - 1]).unwrap_or_default();
    if rust_attribute_name(attribute) == Some("error") {
        return false;
    }

    advance_rust_cursor(bytes, index, line, attribute_end);
    true
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn add_catalog_message(catalog: &mut Catalog, message: &str, reference: String) {
    let entry = catalog.entry(message.to_string()).or_default();
    entry.references.insert(reference);
}

fn render_pot_catalog(catalog: &Catalog, package_name: &str, package_version: &str) -> String {
    render_catalog(catalog, None, package_name, package_version)
}

fn render_po_catalog(
    catalog: &Catalog,
    language: &str,
    package_name: &str,
    package_version: &str,
) -> String {
    render_catalog(catalog, Some(language), package_name, package_version)
}

fn render_catalog(
    catalog: &Catalog,
    language: Option<&str>,
    package_name: &str,
    package_version: &str,
) -> String {
    let mut output = String::new();
    write_po_header(&mut output, language, package_name, package_version);

    for (message, entry) in catalog {
        for reference in &entry.references {
            writeln!(output, "#: {reference}").expect("Failed to format po reference");
        }
        write_po_string_field(&mut output, "msgid", message);
        if let Some(language) = language {
            let _ = language;
            write_po_string_field(&mut output, "msgstr", message);
        } else {
            write_po_string_field(&mut output, "msgstr", "");
        }
        output.push('\n');
    }

    output
}

fn write_po_header(
    output: &mut String,
    language: Option<&str>,
    package_name: &str,
    package_version: &str,
) {
    let language = language.unwrap_or("");
    output.push_str("msgid \"\"\n");
    output.push_str("msgstr \"\"\n");
    output.push_str(&po_wrapped_line(&format!(
        "Project-Id-Version: {package_name} {package_version}\n"
    )));
    output.push_str(&po_wrapped_line("MIME-Version: 1.0\n"));
    output.push_str(&po_wrapped_line(
        "Content-Type: text/plain; charset=UTF-8\n",
    ));
    output.push_str(&po_wrapped_line("Content-Transfer-Encoding: 8bit\n"));
    output.push_str(&po_wrapped_line(&format!("Language: {language}\n")));
    output.push_str(&po_wrapped_line(
        "Plural-Forms: nplurals=2; plural=(n != 1);\n",
    ));
    output.push('\n');
}

fn write_po_string_field(output: &mut String, field: &str, value: &str) {
    if value.is_empty() {
        let _ = writeln!(output, "{field} \"\"");
        return;
    }

    if value.contains('\n') {
        let _ = writeln!(output, "{field} \"\"");
        output.push_str(&po_wrapped_line(value));
        return;
    }

    let _ = writeln!(output, "{field} \"{}\"", escape_po_string(value));
}

fn po_wrapped_line(value: &str) -> String {
    format!("\"{}\"\n", escape_po_string(value))
}

fn escape_po_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn compile_translations(po_dir: &Path, locale_dir: &Path, gettext_domain: &str) -> Vec<String> {
    let locales = discover_po_locales(po_dir);
    for locale in &locales {
        let po_path = po_dir.join(format!("{locale}.po"));
        let mo_path = locale_dir
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{gettext_domain}.mo"));
        let bytes = compile_mo_file(&po_path);
        if let Some(parent) = mo_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|err| panic!("Failed to create {}: {err}", parent.display()));
        }
        write_if_changed(&mo_path, &bytes);
    }
    locales
}

fn discover_po_locales(po_dir: &Path) -> Vec<String> {
    let mut locales = fs::read_dir(po_dir)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", po_dir.display()))
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("po"))
        .filter_map(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    locales.sort();
    locales
}

fn compile_mo_file(path: &Path) -> Vec<u8> {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {err}", path.display()));
    let mut entries = parse_po_entries(&source);
    entries.sort_by(|left, right| {
        let left_key = mo_original_key(left);
        let right_key = mo_original_key(right);
        left_key.cmp(&right_key)
    });

    let count = entries.len() as u32;
    let originals_offset = 28u32;
    let translations_offset = originals_offset + count * 8;
    let originals_data_offset = translations_offset + count * 8;

    let original_keys = entries
        .iter()
        .map(mo_original_key)
        .collect::<Vec<Vec<u8>>>();
    let translated_values = entries
        .iter()
        .map(mo_translation_value)
        .collect::<Vec<Vec<u8>>>();

    let mut original_table = Vec::with_capacity(entries.len());
    let mut translation_table = Vec::with_capacity(entries.len());
    let mut data = Vec::new();
    let mut offset = originals_data_offset;

    for key in &original_keys {
        original_table.push((key.len() as u32, offset));
        data.extend_from_slice(key);
        data.push(0);
        offset += key.len() as u32 + 1;
    }

    for value in &translated_values {
        translation_table.push((value.len() as u32, offset));
        data.extend_from_slice(value);
        data.push(0);
        offset += value.len() as u32 + 1;
    }

    let mut output = Vec::new();
    push_u32_le(&mut output, 0x9504_12de);
    push_u32_le(&mut output, 0);
    push_u32_le(&mut output, count);
    push_u32_le(&mut output, originals_offset);
    push_u32_le(&mut output, translations_offset);
    push_u32_le(&mut output, 0);
    push_u32_le(&mut output, 0);

    for (length, offset) in &original_table {
        push_u32_le(&mut output, *length);
        push_u32_le(&mut output, *offset);
    }
    for (length, offset) in &translation_table {
        push_u32_le(&mut output, *length);
        push_u32_le(&mut output, *offset);
    }

    output.extend_from_slice(&data);
    output
}

fn mo_original_key(entry: &PoEntry) -> Vec<u8> {
    match &entry.msgid_plural {
        Some(msgid_plural) => {
            let mut bytes = entry.msgid.as_bytes().to_vec();
            bytes.push(0);
            bytes.extend_from_slice(msgid_plural.as_bytes());
            bytes
        }
        None => entry.msgid.as_bytes().to_vec(),
    }
}

fn mo_translation_value(entry: &PoEntry) -> Vec<u8> {
    if entry.msgstr_plural.is_empty() {
        return entry.msgstr.as_bytes().to_vec();
    }

    let max_index = entry.msgstr_plural.keys().copied().max().unwrap_or(0);
    let mut bytes = Vec::new();
    for index in 0..=max_index {
        if index > 0 {
            bytes.push(0);
        }
        if let Some(value) = entry.msgstr_plural.get(&index) {
            bytes.extend_from_slice(value.as_bytes());
        }
    }
    bytes
}

fn parse_po_entries(source: &str) -> Vec<PoEntry> {
    let mut entries = Vec::new();
    let mut current = PoEntry::default();
    let mut active_field = None;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() {
            finalize_po_entry(&mut entries, &mut current);
            active_field = None;
            continue;
        }

        if trimmed.starts_with('#') {
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("msgid_plural ") {
            current.msgid_plural = Some(parse_po_quoted(value));
            active_field = Some(ActivePoField::IdPlural);
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("msgid ") {
            if !current.msgid.is_empty()
                || !current.msgstr.is_empty()
                || !current.msgstr_plural.is_empty()
            {
                finalize_po_entry(&mut entries, &mut current);
            }
            current.msgid = parse_po_quoted(value);
            active_field = Some(ActivePoField::Id);
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("msgstr ") {
            current.msgstr = parse_po_quoted(value);
            active_field = Some(ActivePoField::Str);
            continue;
        }

        if let Some((index, value)) = parse_po_plural_msgstr(trimmed) {
            current.msgstr_plural.insert(index, value);
            active_field = Some(ActivePoField::StrPlural(index));
            continue;
        }

        if trimmed.starts_with('"') {
            let value = parse_po_quoted(trimmed);
            match active_field {
                Some(ActivePoField::Id) => current.msgid.push_str(&value),
                Some(ActivePoField::IdPlural) => current
                    .msgid_plural
                    .get_or_insert_with(String::new)
                    .push_str(&value),
                Some(ActivePoField::Str) => current.msgstr.push_str(&value),
                Some(ActivePoField::StrPlural(index)) => current
                    .msgstr_plural
                    .entry(index)
                    .or_default()
                    .push_str(&value),
                None => {}
            }
        }
    }

    finalize_po_entry(&mut entries, &mut current);
    entries
}

fn parse_po_plural_msgstr(line: &str) -> Option<(usize, String)> {
    let remainder = line.strip_prefix("msgstr[")?;
    let closing = remainder.find(']')?;
    let index = remainder[..closing].parse().ok()?;
    let value = remainder[closing + 1..].trim_start();
    Some((index, parse_po_quoted(value)))
}

fn parse_po_quoted(value: &str) -> String {
    let trimmed = value.trim();
    let stripped = trimmed
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or("");
    unescape_po_string(stripped)
}

fn unescape_po_string(value: &str) -> String {
    let mut output = String::new();
    let mut chars = value.chars();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        let Some(escaped) = chars.next() else {
            break;
        };

        match escaped {
            '\\' => output.push('\\'),
            '"' => output.push('"'),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            other => output.push(other),
        }
    }

    output
}

fn finalize_po_entry(entries: &mut Vec<PoEntry>, current: &mut PoEntry) {
    if current.msgid.is_empty() && current.msgstr.is_empty() && current.msgstr_plural.is_empty() {
        return;
    }

    entries.push(std::mem::take(current));
}

fn push_u32_le(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn line_number(source: &str, index: usize) -> usize {
    source[..index]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn source_reference(source_root: &Path, path: &Path, line: usize) -> String {
    let path = path.strip_prefix(source_root).unwrap_or(path);
    format!("{}:{line}", path.display())
}

#[cfg(test)]
mod tests {
    use super::{
        collect_desktop_strings, collect_rust_strings, collect_rust_strings_from_source,
        collect_ui_strings, looks_translatable_rust_string, merge_desktop_translations,
        non_application_rust_source_dir, render_pot_catalog, source_reference, Catalog,
        NON_APPLICATION_RUST_DIRECTORY_NAMES, NON_APPLICATION_RUST_SOURCE_DIRS,
    };
    use std::path::{Path, PathBuf};

    fn application_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("Lifecycle crate should live below the application root")
            .to_path_buf()
    }

    #[test]
    fn catalog_header_uses_explicit_application_package_metadata() {
        let rendered = render_pot_catalog(&Catalog::new(), "application", "1.2.3");
        assert!(rendered.contains("Project-Id-Version: application 1.2.3\\n"));
        assert!(!rendered.contains("keycord-lifecycle"));
    }

    #[test]
    fn source_references_remain_relative_to_the_application_root() {
        assert_eq!(
            source_reference(
                Path::new("/workspace/keycord"),
                Path::new(
                    "/workspace/keycord/crates/keycord-entries/data/window-pages.fragment.ui",
                ),
                42,
            ),
            "crates/keycord-entries/data/window-pages.fragment.ui:42"
        );
    }

    #[test]
    fn rendered_desktop_strings_are_attributed_to_the_tracked_template() {
        let source_root = application_root();
        let relative_path = "crates/keycord-lifecycle/data/keycord.desktop.in";
        let rendered = "[Desktop Entry]\nName=Keycord\nComment=Browse password stores\n";
        let mut catalog = Catalog::new();

        collect_desktop_strings(
            &source_root,
            &source_root.join(relative_path),
            rendered,
            &mut catalog,
        );

        for message in ["Keycord", "Browse password stores"] {
            let entry = catalog
                .get(message)
                .unwrap_or_else(|| panic!("missing rendered desktop message `{message}`"));
            assert!(
                entry
                    .references
                    .iter()
                    .any(|reference| reference.starts_with(relative_path)),
                "`{message}` should be attributed to {relative_path}"
            );
        }
    }

    #[test]
    fn desktop_metadata_includes_the_dutch_application_name() {
        let rendered = merge_desktop_translations(
            &application_root(),
            "[Desktop Entry]\nName=Keycord\nComment=Browse and edit password stores\n",
        );

        assert!(rendered.contains("Name=Keycord\n"));
        assert!(rendered.contains("Name[nl]=Sleutelkoord\n"));
        assert!(rendered.contains("Comment[nl]=Wachtwoordopslagen bekijken en bewerken\n"));
    }

    #[test]
    fn tooling_sources_are_excluded_from_application_translations() {
        let source_root = application_root();
        for relative_path in NON_APPLICATION_RUST_SOURCE_DIRS {
            assert!(non_application_rust_source_dir(
                &source_root,
                &source_root.join(relative_path)
            ));
        }

        let mut catalog = Catalog::new();
        collect_rust_strings(&source_root, &source_root.join("crates"), &mut catalog);
        for entry in catalog.values() {
            for reference in &entry.references {
                assert!(
                    NON_APPLICATION_RUST_SOURCE_DIRS
                        .iter()
                        .all(|excluded| !reference.starts_with(excluded)),
                    "tool-only source leaked into translations: {reference}"
                );
                assert!(
                    NON_APPLICATION_RUST_DIRECTORY_NAMES
                        .iter()
                        .all(|directory| !reference.split('/').any(|part| part == *directory)),
                    "non-application source leaked into translations: {reference}"
                );
            }
        }
    }

    #[test]
    fn machine_readable_rust_literals_are_not_translatable() {
        for value in [
            "-----BEGIN PGP PRIVATE KEY BLOCK-----",
            "-----END PGP PRIVATE KEY BLOCK-----",
            "-----BEGIN PGP SIGNATURE-----",
            "-----BEGIN SSH SIGNATURE-----",
            "--format=%H%x00%h%x00%s",
            "*.{extension}",
            "flatpak override --user --device=all {app_id}",
            "git rev-parse -q --verify HEAD^{commit}",
            "GIT_{env_prefix}_NAME",
            "HEAD^{commit}",
            "entry.open-new-window",
            "GetInitialResultSet",
            "keycord.toml",
            "update-{:032x}",
            "{app_id}-passkey.xml",
            "{PASSKEY_FIELD_KEY}: {storage_value}",
            "stdin: [redacted]",
            "stdin: provided",
            "passkey: [redacted]",
            "find \"{}\" is \"{}\"",
            "{context}\n$ {command}\nstatus: {status}",
            "<a href=\"{}\">{}</a>",
            "@define-color accent_bg_color {background};",
            "D:P(A;;FA;;;SY)",
            "username:\nemail:\nurl:",
        ] {
            assert!(!looks_translatable_rust_string(value), "leaked `{value}`");
        }

        for value in [
            "This item does not contain an armored private key.",
            "Run the Git command again.",
            "The pass command failed.",
            "Choose which folder-specific .gpg-id file to manage.",
            "Step {current}/{total}: touch your key if it blinks.",
            "The value is [redacted].",
            "{name}: {status}",
            "<b>Important warning</b>",
        ] {
            assert!(
                looks_translatable_rust_string(value),
                "hid user-facing `{value}`"
            );
        }
    }

    #[test]
    fn rust_test_and_attribute_literals_are_not_translatable() {
        let source_root = Path::new("/workspace/keycord");
        let path = source_root.join("src/example.rs");
        let source = r##"
const VISIBLE: &str = "Visible user-facing message.";

#[expect(dead_code, reason = "Technical lint reason")]
const UNUSED: &str = "Another visible user-facing message.";

#[some_attribute('a, reason = "Technical lifetime attribute reason")]
const ATTRIBUTE_LIFETIME: &str = "Visible after a lifetime attribute.";

#[error ("lowercase user-facing error.")]
struct Error;

#[cfg(not(test))]
const NOT_TEST: &str = "Visible behind not test.";

#[cfg(feature = "test-support")]
const TEST_SUPPORT: &str = "Visible behind a test-support feature.";

#[cfg(any(test, target_os = "linux"))]
const SOMETIMES_PRODUCTION: &str = "Visible behind an any predicate.";

#[cfg(all(unix, test))]
const ONLY_TEST: &str = "Technical all-test literal";

#[test]
fn standalone_test() {
    let closing_brace = "}";
    let raw_closing_brace = r#"}"#;
    let closing_brace_char = '}';
    let escaped_closing_brace_char = '\u{7d}';
    // }
    /* nested { /* } */ } */
    assert!(false, "Technical test assertion after braces");
}

const AFTER_TEST: &str = "Visible after the standalone test.";

fn technical_contexts(message: &str) {
    let _ = message.contains("Technical matcher needle");
    let _ = message_contains_any(message, &["Technical array needle"]);
    let _ = Regex::new("pattern").expect("Technical expectation");
    unreachable!("Technical unreachable message");
    log_info(format!("Technical log message: {message}"));
    log_info(gettext("Explicitly translated log message."));
    println!("Technical console message: {message}");
    eprintln!("Technical console detail: {}", "Technical nested console argument");
}
"##;
        let mut catalog = Catalog::new();

        collect_rust_strings_from_source(source_root, &path, source, &mut catalog);

        assert!(catalog.contains_key("Visible user-facing message."));
        assert!(catalog.contains_key("Another visible user-facing message."));
        assert!(catalog.contains_key("Visible after a lifetime attribute."));
        assert!(catalog.contains_key("lowercase user-facing error."));
        assert!(catalog.contains_key("Visible behind not test."));
        assert!(catalog.contains_key("Visible behind a test-support feature."));
        assert!(catalog.contains_key("Visible behind an any predicate."));
        assert!(catalog.contains_key("Visible after the standalone test."));
        assert!(catalog.contains_key("Explicitly translated log message."));
        assert!(!catalog.contains_key("Technical lint reason"));
        assert!(!catalog.contains_key("Technical lifetime attribute reason"));
        assert!(!catalog.contains_key("Technical all-test literal"));
        assert!(!catalog.contains_key("Technical test assertion after braces"));
        assert!(!catalog.contains_key("Technical matcher needle"));
        assert!(!catalog.contains_key("Technical array needle"));
        assert!(!catalog.contains_key("Technical expectation"));
        assert!(!catalog.contains_key("Technical unreachable message"));
        assert!(!catalog.contains_key("Technical log message: {message}"));
        assert!(!catalog.contains_key("Technical console message: {message}"));
        assert!(!catalog.contains_key("Technical console detail: {}"));
        assert!(!catalog.contains_key("Technical nested console argument"));
    }

    #[test]
    fn split_window_strings_are_attributed_to_the_owning_crate() {
        let source_root = application_root();
        let cases = [
            (
                "crates/keycord-entries/data/window-pages.fragment.ui",
                "Text Editor",
            ),
            (
                "crates/keycord-preferences/data/window-page.fragment.ui",
                "Search preferences",
            ),
            (
                "crates/keycord-docs/data/window-pages.fragment.ui",
                "Search docs",
            ),
            (
                "crates/keycord-git/data/window-audit-page.fragment.ui",
                "Inspect change history",
            ),
            (
                "crates/keycord-stores/data/window-pages.fragment.ui",
                "Import passwords",
            ),
            (
                "crates/keycord-keys/data/window-generation-pages.fragment.ui",
                "Generate private key",
            ),
            (
                "crates/keycord-entries/data/shortcuts-sections.fragment.ui",
                "Pass files",
            ),
            (
                "crates/keycord-git/data/shortcuts-list-sync-item.fragment.ui",
                "Sync stores",
            ),
            (
                "crates/keycord-stores/data/shortcuts-section.fragment.ui",
                "Stores",
            ),
            (
                "crates/keycord-docs/data/shortcuts-tool-item.fragment.ui",
                "Open docs",
            ),
            (
                "crates/keycord-preferences/data/shortcuts-general-item.fragment.ui",
                "Open preferences",
            ),
        ];

        for (relative_path, message) in cases {
            let mut catalog = Catalog::new();
            collect_ui_strings(&source_root, &source_root.join(relative_path), &mut catalog);
            let entry = catalog
                .get(message)
                .unwrap_or_else(|| panic!("missing `{message}` from {relative_path}"));
            assert!(
                entry
                    .references
                    .iter()
                    .any(|reference| reference.starts_with(relative_path)),
                "`{message}` should be attributed to {relative_path}"
            );
        }
    }
}
