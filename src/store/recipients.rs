use crate::backend::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
use crate::i18n::gettext;
use std::fs;
use std::path::{Component, Path, PathBuf};
#[cfg(test)]
use std::{cell::RefCell, rc::Rc};
use walkdir::WalkDir;

const REQUIRE_ALL_PRIVATE_KEYS_METADATA: &str = "keycord-private-key-requirement=all";
pub const ROOT_STORE_RECIPIENTS_SCOPE: &str = ".";

fn normalized_store_recipients_scope(scope: &str) -> String {
    let trimmed = scope.trim();
    if trimmed.is_empty() || trimmed == ROOT_STORE_RECIPIENTS_SCOPE {
        return ROOT_STORE_RECIPIENTS_SCOPE.to_string();
    }

    let mut relative = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            _ => return ROOT_STORE_RECIPIENTS_SCOPE.to_string(),
        }
    }

    if relative.as_os_str().is_empty() {
        ROOT_STORE_RECIPIENTS_SCOPE.to_string()
    } else {
        relative.to_string_lossy().to_string()
    }
}

fn store_recipients_scope_directory(store_root: &str, scope: &str) -> PathBuf {
    let normalized = normalized_store_recipients_scope(scope);
    let mut path = PathBuf::from(store_root);
    if normalized != ROOT_STORE_RECIPIENTS_SCOPE {
        path.push(normalized);
    }
    path
}

fn standard_recipients_path_for_scope(store_root: &str, scope: &str) -> PathBuf {
    store_recipients_scope_directory(store_root, scope).join(".gpg-id")
}

pub fn read_store_standard_recipients(store_root: &str) -> Vec<String> {
    read_store_standard_recipients_for_scope(store_root, ROOT_STORE_RECIPIENTS_SCOPE)
}

pub fn read_store_standard_recipients_for_scope(store_root: &str, scope: &str) -> Vec<String> {
    let path = standard_recipients_path_for_scope(store_root, scope);
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };

    parse_standard_recipients(&contents)
}

pub fn read_store_recipients(store_root: &str) -> Vec<String> {
    read_store_standard_recipients(store_root)
}

pub fn read_store_recipients_for_scope(store_root: &str, scope: &str) -> Vec<String> {
    read_store_standard_recipients_for_scope(store_root, scope)
}

pub const fn store_is_supported_in_current_build(_store_root: &str) -> bool {
    true
}

pub fn read_store_private_key_requirement(
    store_root: &str,
) -> StoreRecipientsPrivateKeyRequirement {
    read_store_private_key_requirement_for_scope(store_root, ROOT_STORE_RECIPIENTS_SCOPE)
}

pub fn read_store_private_key_requirement_for_scope(
    store_root: &str,
    scope: &str,
) -> StoreRecipientsPrivateKeyRequirement {
    let path = standard_recipients_path_for_scope(store_root, scope);
    let Ok(contents) = fs::read_to_string(path) else {
        return StoreRecipientsPrivateKeyRequirement::AnyManagedKey;
    };

    for line in contents.lines() {
        if line
            .trim()
            .strip_prefix('#')
            .map(str::trim)
            .is_some_and(|value| value.eq_ignore_ascii_case(REQUIRE_ALL_PRIVATE_KEYS_METADATA))
        {
            return StoreRecipientsPrivateKeyRequirement::AllManagedKeys;
        }
    }

    StoreRecipientsPrivateKeyRequirement::AnyManagedKey
}

fn root_store_standard_recipients_contents(store_root: &str) -> String {
    fs::read_to_string(standard_recipients_path_for_scope(
        store_root,
        ROOT_STORE_RECIPIENTS_SCOPE,
    ))
    .unwrap_or_default()
}

fn store_recipients_scope_from_path(store_root: &Path, recipients_path: &Path) -> Option<String> {
    let directory = recipients_path.parent()?;
    let relative = directory.strip_prefix(store_root).ok()?;
    if relative.as_os_str().is_empty() {
        Some(ROOT_STORE_RECIPIENTS_SCOPE.to_string())
    } else {
        Some(relative.to_string_lossy().to_string())
    }
}

pub fn relevant_store_recipient_scopes(store_root: &str) -> Vec<String> {
    let store_root = Path::new(store_root);
    let root_path = store_root.join(".gpg-id");
    let root_contents =
        root_store_standard_recipients_contents(store_root.to_string_lossy().as_ref());
    let mut scopes = Vec::new();

    if root_path.is_file() {
        scopes.push(ROOT_STORE_RECIPIENTS_SCOPE.to_string());
    }

    for entry in WalkDir::new(store_root).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() || entry.file_name() != ".gpg-id" {
            continue;
        }
        if entry.path() == root_path {
            continue;
        }
        let Ok(contents) = fs::read_to_string(entry.path()) else {
            continue;
        };
        if contents == root_contents {
            continue;
        }
        let Some(scope) = store_recipients_scope_from_path(store_root, entry.path()) else {
            continue;
        };
        if scopes.iter().any(|existing| existing == &scope) {
            continue;
        }
        scopes.push(scope);
    }

    scopes.sort_by(|left, right| {
        if left == ROOT_STORE_RECIPIENTS_SCOPE {
            std::cmp::Ordering::Less
        } else if right == ROOT_STORE_RECIPIENTS_SCOPE {
            std::cmp::Ordering::Greater
        } else {
            left.cmp(right)
        }
    });
    scopes
}

pub fn store_recipients_subtitle(store_root: &str) -> String {
    let recipients = read_store_recipients(store_root);
    match recipients.len() {
        0 => gettext("No recipients set"),
        1 => gettext("1 recipient"),
        count => gettext("{count} recipients").replace("{count}", &count.to_string()),
    }
}

fn push_unique_recipient(recipients: &mut Vec<String>, recipient: String) {
    if recipient.is_empty() || recipients.iter().any(|existing| existing == &recipient) {
        return;
    }

    recipients.push(recipient);
}

pub fn split_store_recipients(recipients: &[String]) -> StoreRecipients {
    let mut standard = Vec::new();

    for recipient in recipients {
        push_unique_recipient(&mut standard, recipient.clone());
    }

    StoreRecipients::new(standard)
}

#[cfg(test)]
pub fn append_standard_recipients(recipients: &Rc<RefCell<Vec<String>>>, input: &str) -> bool {
    let parsed = parse_standard_recipients(input);
    if parsed.is_empty() {
        return false;
    }

    let mut values = recipients.borrow_mut();
    let original_len = values.len();
    for recipient in parsed {
        push_unique_recipient(&mut values, recipient);
    }

    values.len() > original_len
}

pub fn parse_standard_recipients(value: &str) -> Vec<String> {
    let mut recipients = Vec::new();

    for line in value.lines() {
        for recipient in line.split([',', ';']) {
            let recipient = recipient
                .split_once('#')
                .map_or(recipient, |(value, _)| value);
            let recipient = recipient.trim();
            let recipient = normalize_standard_recipient(recipient);
            push_unique_recipient(&mut recipients, recipient);
        }
    }

    recipients
}

pub fn normalize_standard_recipient(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let compact = trimmed
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect::<String>();
    if trimmed.contains(char::is_whitespace) && compact.chars().all(|c| c.is_ascii_hexdigit()) {
        compact
    } else {
        trimmed.to_string()
    }
}

pub fn stores_with_preferred_first(stores: &[String], preferred: &str) -> Vec<String> {
    let mut ordered = vec![preferred.to_string()];
    for store in stores {
        if store != preferred {
            ordered.push(store.clone());
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::{
        append_standard_recipients, normalize_standard_recipient, parse_standard_recipients,
        read_store_private_key_requirement_for_scope, read_store_recipients_for_scope,
        relevant_store_recipient_scopes, split_store_recipients,
        store_is_supported_in_current_build, store_recipients_subtitle,
        stores_with_preferred_first, ROOT_STORE_RECIPIENTS_SCOPE,
    };
    use crate::backend::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
    use crate::i18n::gettext;
    use std::{
        cell::RefCell,
        fs,
        rc::Rc,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn standard_recipients_are_trimmed_and_deduplicated() {
        assert_eq!(
            parse_standard_recipients("alice@example.com; bob@example.com,\nalice@example.com"),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string()
            ]
        );
    }

    #[test]
    fn standard_fingerprints_drop_internal_spaces() {
        assert_eq!(
            normalize_standard_recipient("7D FF 03 8D EE 12 AB 34"),
            "7DFF038DEE12AB34".to_string()
        );
    }

    #[test]
    fn standard_user_ids_keep_internal_spaces() {
        assert_eq!(
            normalize_standard_recipient("Alice Example <alice@example.com>"),
            "Alice Example <alice@example.com>".to_string()
        );
    }

    #[test]
    fn standard_recipient_comments_are_ignored() {
        assert_eq!(
            parse_standard_recipients(
                "# keycord-private-key-requirement=all\nalice@example.com # preferred\nbob@example.com"
            ),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string()
            ]
        );
    }

    #[test]
    fn scope_reads_use_the_requested_relative_directory() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let store = std::env::temp_dir().join(format!("keycord-store-scope-{timestamp}"));
        fs::create_dir_all(store.join("team")).expect("create store directories");
        fs::write(store.join(".gpg-id"), "root@example.com\n").expect("write root recipients");
        fs::write(
            store.join("team/.gpg-id"),
            "# keycord-private-key-requirement=all\nnested@example.com\n",
        )
        .expect("write nested recipients");

        assert_eq!(
            read_store_recipients_for_scope(
                store.to_string_lossy().as_ref(),
                ROOT_STORE_RECIPIENTS_SCOPE
            ),
            vec!["root@example.com".to_string()]
        );
        assert_eq!(
            read_store_recipients_for_scope(store.to_string_lossy().as_ref(), "team"),
            vec!["nested@example.com".to_string()]
        );
        assert_eq!(
            read_store_private_key_requirement_for_scope(store.to_string_lossy().as_ref(), "team"),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        );

        fs::remove_dir_all(store).expect("remove temporary store");
    }

    #[test]
    fn relevant_scopes_include_only_root_and_nested_files_that_differ_from_root() {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        let store = std::env::temp_dir().join(format!("keycord-store-relevant-scopes-{timestamp}"));
        fs::create_dir_all(store.join("team/shared")).expect("create nested store directories");
        fs::create_dir_all(store.join("team/custom"))
            .expect("create second nested store directory");
        fs::write(store.join(".gpg-id"), "root@example.com\n").expect("write root recipients");
        fs::write(store.join("team/shared/.gpg-id"), "root@example.com\n")
            .expect("write identical nested recipients");
        fs::write(store.join("team/custom/.gpg-id"), "custom@example.com\n")
            .expect("write custom nested recipients");

        assert_eq!(
            relevant_store_recipient_scopes(store.to_string_lossy().as_ref()),
            vec![
                ROOT_STORE_RECIPIENTS_SCOPE.to_string(),
                "team/custom".to_string()
            ]
        );

        fs::remove_dir_all(store).expect("remove temporary store");
    }

    #[test]
    fn standard_recipient_input_appends_unique_values() {
        let recipients = Rc::new(RefCell::new(vec!["alice@example.com".to_string()]));

        assert!(append_standard_recipients(
            &recipients,
            "alice@example.com; bob@example.com, carol@example.com"
        ));
        assert_eq!(
            recipients.borrow().clone(),
            vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string(),
                "carol@example.com".to_string()
            ]
        );
    }

    #[test]
    fn store_recipients_are_standard_only() {
        let recipients = vec!["alice@example.com".to_string()];

        assert_eq!(
            split_store_recipients(&recipients),
            StoreRecipients::new(vec!["alice@example.com".to_string()])
        );
    }

    #[test]
    fn preferred_store_moves_to_the_front_once() {
        let stores = vec![
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
            "/tmp/three".to_string(),
        ];
        assert_eq!(
            stores_with_preferred_first(&stores, "/tmp/two"),
            vec![
                "/tmp/two".to_string(),
                "/tmp/one".to_string(),
                "/tmp/three".to_string()
            ]
        );
    }

    #[test]
    fn stores_are_supported_after_fido_store_purge() {
        assert!(store_is_supported_in_current_build("/tmp/store"));
    }

    #[test]
    fn store_recipient_subtitle_counts_standard_recipients() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let store_root = std::env::temp_dir().join(format!("keycord-store-subtitle-{unique}"));
        fs::create_dir_all(&store_root).expect("store root should be created");
        fs::write(store_root.join(".gpg-id"), "alice@example.com\n").expect("write recipients");

        assert_eq!(
            store_recipients_subtitle(store_root.to_str().expect("utf8 temp path")),
            gettext("1 recipient")
        );

        let _ = fs::remove_dir_all(store_root);
    }
}
