//! State and policy for store-recipient page orchestration.

use keycord_runtime::i18n::gettext;

use crate::recipients::ROOT_STORE_RECIPIENTS_SCOPE;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreRecipientsMode {
    Create,
    Edit,
}

impl StoreRecipientsMode {
    pub const fn page_title(self) -> &'static str {
        match self {
            Self::Create => "New Store",
            Self::Edit => "Store keys",
        }
    }

    pub const fn empty_state_subtitle(self) -> &'static str {
        match self {
            Self::Create => "Add at least one recipient to create this store.",
            Self::Edit => "Add at least one recipient to keep saving changes.",
        }
    }

    pub const fn save_failure_message(self) -> &'static str {
        match self {
            Self::Create => "Couldn't create the store.",
            Self::Edit => "Couldn't save store keys.",
        }
    }

    pub const fn creates_store(self) -> bool {
        matches!(self, Self::Create)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreRecipientsRequest {
    pub store: String,
    pub mode: StoreRecipientsMode,
}

impl StoreRecipientsRequest {
    pub fn create(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            mode: StoreRecipientsMode::Create,
        }
    }

    pub fn edit(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            mode: StoreRecipientsMode::Edit,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreRecipientsSelectionMode {
    Empty,
    StandardOnly,
}

impl StoreRecipientsSelectionMode {
    pub const fn allows_standard_recipients(self) -> bool {
        matches!(self, Self::Empty | Self::StandardOnly)
    }

    pub const fn standard_action_block_message(self) -> Option<&'static str> {
        None
    }
}

pub const fn store_recipients_selection_mode(
    recipients: &[String],
) -> StoreRecipientsSelectionMode {
    if recipients.is_empty() {
        StoreRecipientsSelectionMode::Empty
    } else {
        StoreRecipientsSelectionMode::StandardOnly
    }
}

pub const fn show_standard_private_key_choice(
    _selection_mode: StoreRecipientsSelectionMode,
    _active: bool,
) -> bool {
    true
}

pub const fn should_reschedule_after_finish(
    save_queued: bool,
    include_dirty: bool,
    recipients_dirty: bool,
) -> bool {
    save_queued || (include_dirty && recipients_dirty)
}

pub fn should_refresh_after_save(
    current_request: Option<&StoreRecipientsRequest>,
    saved_store: &str,
    recipients_dirty: bool,
) -> bool {
    !recipients_dirty && current_request.is_some_and(|request| request.store == saved_store)
}

pub fn recipient_matches_parts(recipient: &str, fingerprint: &str, user_ids: &[String]) -> bool {
    let recipient = recipient.trim();
    recipient.eq_ignore_ascii_case(fingerprint)
        || user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

pub fn set_private_key_recipient_values(
    recipients: &mut Vec<String>,
    fingerprint: &str,
    user_ids: &[String],
    enabled: bool,
) -> bool {
    let before = recipients.clone();
    recipients.retain(|value| !recipient_matches_parts(value, fingerprint, user_ids));
    if enabled {
        recipients.push(fingerprint.to_string());
    }
    *recipients != before
}

pub const fn private_key_delete_block_message(
    active: bool,
    require_all_selected_keys: bool,
    selected_available_keys: usize,
) -> Option<&'static str> {
    if !active {
        None
    } else if require_all_selected_keys {
        Some("This selected key is required while all selected private keys are required.")
    } else if selected_available_keys <= 1 {
        Some("Keep another selected private key available before removing this key.")
    } else {
        None
    }
}

pub const fn private_key_toggle_block_message(
    active: bool,
    usable: bool,
    require_all_selected_keys: bool,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) -> Option<&'static str> {
    if !active {
        None
    } else if require_all_selected_keys {
        Some("Keep this key selected while all selected private keys are required.")
    } else if selected_available_keys <= 1 {
        Some("Keep at least one selected private key available.")
    } else if usable && selected_usable_keys <= 1 {
        Some("Unlock another selected private key before clearing this one.")
    } else {
        None
    }
}

pub const fn show_recipient_scope_selector(scopes: &[String]) -> bool {
    scopes.len() > 1
}

pub fn recipient_scope_label(scope: &str) -> String {
    if scope == ROOT_STORE_RECIPIENTS_SCOPE {
        gettext("Default")
    } else {
        scope.to_string()
    }
}

pub const fn show_require_all_private_keys_option(
    _selection_mode: StoreRecipientsSelectionMode,
    has_keys: bool,
) -> bool {
    has_keys
}

pub const fn show_store_options_title_above_git_row(
    show_options_group: bool,
    show_git: bool,
) -> bool {
    show_git && !show_options_group
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_modes_keep_creation_and_edit_contracts() {
        assert_eq!(StoreRecipientsMode::Create.page_title(), "New Store");
        assert!(StoreRecipientsMode::Create.creates_store());
        assert_eq!(
            StoreRecipientsMode::Edit.save_failure_message(),
            "Couldn't save store keys."
        );
        assert!(!StoreRecipientsMode::Edit.creates_store());
    }

    #[test]
    fn selection_policy_tracks_empty_and_standard_recipient_lists() {
        assert_eq!(
            store_recipients_selection_mode(&[]),
            StoreRecipientsSelectionMode::Empty
        );
        assert_eq!(
            store_recipients_selection_mode(&["alice@example.com".to_string()]),
            StoreRecipientsSelectionMode::StandardOnly
        );
        assert!(show_standard_private_key_choice(
            StoreRecipientsSelectionMode::StandardOnly,
            false
        ));
    }

    #[test]
    fn recipient_updates_replace_aliases_with_the_fingerprint() {
        let mut recipients = vec!["Alice <alice@example.com>".to_string()];
        assert!(set_private_key_recipient_values(
            &mut recipients,
            "AAAA",
            &["Alice <alice@example.com>".to_string()],
            true,
        ));
        assert_eq!(recipients, ["AAAA"]);
    }

    #[test]
    fn save_queue_and_refresh_policy_is_stable() {
        assert!(should_reschedule_after_finish(true, false, false));
        assert!(should_reschedule_after_finish(false, true, true));
        let request = StoreRecipientsRequest::edit("/tmp/store");
        assert!(should_refresh_after_save(
            Some(&request),
            "/tmp/store",
            false
        ));
        assert!(!should_refresh_after_save(
            Some(&request),
            "/tmp/store",
            true
        ));
    }

    #[test]
    fn last_key_and_require_all_rules_prevent_destructive_toggles() {
        assert!(private_key_delete_block_message(true, false, 1).is_some());
        assert!(private_key_toggle_block_message(true, true, true, 2, 2).is_some());
        assert_eq!(
            private_key_toggle_block_message(true, true, false, 2, 2),
            None
        );
    }

    #[test]
    fn scope_and_options_visibility_follow_available_content() {
        assert!(!show_recipient_scope_selector(&[".".to_string()]));
        assert!(show_recipient_scope_selector(&[
            ".".to_string(),
            "team".to_string()
        ]));
        assert_eq!(recipient_scope_label("."), "Default");
        assert!(show_store_options_title_above_git_row(false, true));
    }
}
