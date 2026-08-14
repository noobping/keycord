//! Store-list and management policy shared by the UI and composition shell.

use std::fs;
use std::io;
use std::path::Path;

pub const NUMBERED_STORE_SHORTCUT_COUNT: usize = 6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedStoreFolderMode {
    AddExisting,
    CreateNew,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PassImportSourceState {
    #[default]
    Checking,
    Unavailable,
    Available(Vec<String>),
}

impl PassImportSourceState {
    pub const fn is_available(&self) -> bool {
        matches!(self, Self::Available(sources) if !sources.is_empty())
    }

    pub fn sources(&self) -> Option<&[String]> {
        match self {
            Self::Available(sources) if !sources.is_empty() => Some(sources),
            Self::Checking | Self::Unavailable | Self::Available(_) => None,
        }
    }
}

pub fn updated_stores_after_add(stores: &[String], new_store: &str) -> Option<Vec<String>> {
    if stores.iter().any(|store| store == new_store) {
        return None;
    }
    let mut updated = stores.to_vec();
    updated.push(new_store.to_string());
    Some(updated)
}

pub fn updated_stores_after_delete(
    stores: &[String],
    store_to_remove: &str,
) -> Option<Vec<String>> {
    let position = stores.iter().position(|store| store == store_to_remove)?;
    let mut updated = stores.to_vec();
    updated.remove(position);
    Some(updated)
}

pub const fn initial_recipients_for_store_creation(
    existing_recipients: Vec<String>,
) -> Vec<String> {
    existing_recipients
}

pub fn configured_store_for_shortcut_slot(stores: &[String], slot: usize) -> Option<String> {
    if !(1..=NUMBERED_STORE_SHORTCUT_COUNT).contains(&slot) {
        return None;
    }
    stores.get(slot - 1).cloned()
}

pub const fn selected_store_folder_mode(is_empty: bool) -> SelectedStoreFolderMode {
    if is_empty {
        SelectedStoreFolderMode::CreateNew
    } else {
        SelectedStoreFolderMode::AddExisting
    }
}

pub fn folder_is_empty(path: &str) -> io::Result<bool> {
    let path = Path::new(path);
    if !path.exists() {
        return Ok(true);
    }
    Ok(fs::read_dir(path)?.next().is_none())
}

pub const fn empty_store_list_text() -> (&'static str, &'static str) {
    ("No password stores", "Add a folder.")
}

pub fn clone_url_dialog_error_message(url: &str) -> Option<&'static str> {
    url.trim().is_empty().then_some("Enter a repository URL.")
}

pub const fn pass_import_row_enabled(
    uses_host_command_backend: bool,
    stores: &[String],
    source_state: &PassImportSourceState,
) -> bool {
    uses_host_command_backend && !stores.is_empty() && source_state.is_available()
}

pub const fn pass_import_row_subtitle(
    uses_host_command_backend: bool,
    stores: &[String],
    source_state: &PassImportSourceState,
) -> &'static str {
    if !uses_host_command_backend {
        "Switch Backend to Host to use pass import."
    } else if stores.is_empty() {
        "Add a store to use pass import."
    } else if source_state.is_available() {
        "Use pass import with your custom pass command."
    } else if matches!(source_state, PassImportSourceState::Checking) {
        "Checking pass import availability."
    } else {
        "pass import is not available."
    }
}

pub const fn import_source_subtitle(source_path: Option<&str>) -> &'static str {
    if source_path.is_some() {
        ""
    } else {
        "Choose a file or folder if the importer needs one."
    }
}

pub fn normalize_optional_import_text(text: &str) -> Option<String> {
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        clone_url_dialog_error_message, configured_store_for_shortcut_slot, empty_store_list_text,
        initial_recipients_for_store_creation, pass_import_row_enabled, pass_import_row_subtitle,
        selected_store_folder_mode, updated_stores_after_add, updated_stores_after_delete,
        PassImportSourceState, SelectedStoreFolderMode,
    };

    #[test]
    fn list_updates_are_stable_and_duplicate_free() {
        let stores = vec!["/tmp/one".to_string(), "/tmp/two".to_string()];
        assert_eq!(updated_stores_after_add(&stores, "/tmp/one"), None);
        assert_eq!(
            updated_stores_after_add(&stores, "/tmp/three"),
            Some(vec![
                "/tmp/one".to_string(),
                "/tmp/two".to_string(),
                "/tmp/three".to_string(),
            ])
        );
        assert_eq!(
            updated_stores_after_delete(&stores, "/tmp/one"),
            Some(vec!["/tmp/two".to_string()])
        );
    }

    #[test]
    fn creation_and_shortcut_policy_matches_store_order() {
        assert_eq!(
            selected_store_folder_mode(true),
            SelectedStoreFolderMode::CreateNew
        );
        assert_eq!(
            selected_store_folder_mode(false),
            SelectedStoreFolderMode::AddExisting
        );
        assert_eq!(
            initial_recipients_for_store_creation(vec!["alice@example.com".to_string()]),
            vec!["alice@example.com".to_string()]
        );
        let stores = (1..=7)
            .map(|index| format!("/tmp/{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            configured_store_for_shortcut_slot(&stores, 6),
            Some("/tmp/6".to_string())
        );
        assert_eq!(configured_store_for_shortcut_slot(&stores, 7), None);
        assert_eq!(
            empty_store_list_text(),
            ("No password stores", "Add a folder.")
        );
    }

    #[test]
    fn clone_requires_a_nonempty_repository_url() {
        assert_eq!(
            clone_url_dialog_error_message("  "),
            Some("Enter a repository URL.")
        );
        assert_eq!(
            clone_url_dialog_error_message("ssh://example/repo.git"),
            None
        );
    }

    #[test]
    fn import_row_requires_backend_store_and_source() {
        let available = PassImportSourceState::Available(vec!["bitwarden".to_string()]);
        assert!(!pass_import_row_enabled(
            false,
            &["store".to_string()],
            &available
        ));
        assert!(!pass_import_row_enabled(true, &[], &available));
        assert!(pass_import_row_enabled(
            true,
            &["store".to_string()],
            &available
        ));
        assert_eq!(
            pass_import_row_subtitle(
                true,
                &["store".to_string()],
                &PassImportSourceState::Checking
            ),
            "Checking pass import availability."
        );
    }
}
