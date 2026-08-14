//! Keys-owned private-key inventory and recipient-list presentation.

use super::KeyManagementUiState;
use crate::{
    armored_managed_key_material, is_ripasso_private_key_unlocked, list_connected_smartcard_keys,
    list_ripasso_private_keys, remove_ripasso_private_key,
    ripasso_private_key_requires_session_unlock, ConnectedSmartcardKey, HostGpgPrivateKeySummary,
    ManagedRipassoPrivateKey,
};
use adw::prelude::*;
use adw::{ActionRow, Toast};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::ui::{
    add_persistent_hide_button_with, add_tracked_preferences_group_child, append_info_group_row,
    clear_tracked_preferences_group, dim_label_icon, flat_icon_button_with_tooltip,
};
use std::collections::HashSet;
use std::rc::Rc;

const HOST_GPG_WARNING_NOTICE_ID: &str = "store-recipients-host-gpg-warning";
const RECIPIENT_KEYS_GROUP_TITLE: &str = "Keys for this store";
const RECIPIENT_KEYS_GROUP_DESCRIPTION: &str =
    "Select the private keys that can unlock passwords in this store.";

pub type RecipientKeyMatcher = Rc<dyn Fn(&str, &str, &[String]) -> bool>;
pub type RecipientKeyChoiceVisibility = Rc<dyn Fn(bool) -> bool>;
pub type RecipientKeyToggleMessage = Rc<dyn Fn(bool, bool, usize, usize) -> Option<String>>;
pub type RecipientKeyDeleteMessage = Rc<dyn Fn(bool, usize) -> Option<String>>;
pub type RecipientKeyToggle = Rc<dyn Fn(String, Vec<String>, bool)>;
pub type BeforeRecipientKeyRows = Rc<dyn Fn(bool, Vec<String>)>;

/// Stores-owned recipient policy consumed by the Keys-owned list controller.
#[derive(Clone)]
pub struct RecipientKeyListPolicy {
    pub recipient_matches: RecipientKeyMatcher,
    pub show_choice: RecipientKeyChoiceVisibility,
    pub toggle_blocked_message: RecipientKeyToggleMessage,
    pub delete_blocked_message: RecipientKeyDeleteMessage,
    pub on_toggle: RecipientKeyToggle,
    pub before_key_rows: BeforeRecipientKeyRows,
}

/// Per-refresh Store context without Store domain types or a Stores dependency.
pub struct RecipientKeyListContext {
    pub current_recipients: Vec<String>,
    pub uses_integrated_backend: bool,
    pub uses_host_backend: bool,
    pub policy: RecipientKeyListPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AvailableRecipientKey {
    Managed(ManagedRipassoPrivateKey),
    ConnectedSmartcard(ConnectedSmartcardKey),
    HostOnly(HostGpgPrivateKeySummary),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecipientKeyVerificationWarning {
    HostInspectionFailed,
    SyncDisabled,
}

impl RecipientKeyVerificationWarning {
    const fn title(self) -> &'static str {
        match self {
            Self::HostInspectionFailed => "Couldn't inspect host GPG keys",
            Self::SyncDisabled => "Private keys can't be verified",
        }
    }

    const fn subtitle(self) -> &'static str {
        match self {
            Self::HostInspectionFailed => "Valid host keys may appear unavailable here.",
            Self::SyncDisabled => {
                "Valid host keys may appear unavailable here while private-key sync is off."
            }
        }
    }
}

impl AvailableRecipientKey {
    fn fingerprint(&self) -> &str {
        match self {
            Self::Managed(key) => &key.fingerprint,
            Self::ConnectedSmartcard(key) => &key.fingerprint,
            Self::HostOnly(key) => &key.fingerprint,
        }
    }

    fn user_ids(&self) -> &[String] {
        match self {
            Self::Managed(key) => &key.user_ids,
            Self::ConnectedSmartcard(key) => &key.user_ids,
            Self::HostOnly(key) => &key.user_ids,
        }
    }

    fn title(&self) -> String {
        match self {
            Self::Managed(key) => key.title(),
            Self::ConnectedSmartcard(key) => key.title(),
            Self::HostOnly(key) => key.title(),
        }
    }
}

pub(super) fn connect_recipient_warning_control(state: &KeyManagementUiState) {
    let warning_group = state.widgets.recipient_host_gpg_warning_group.clone();
    let hide_notice = state.ports.hide_notice.clone();
    add_persistent_hide_button_with(
        &state.widgets.recipient_host_gpg_warning_row,
        HOST_GPG_WARNING_NOTICE_ID,
        move |notice_id| hide_notice(notice_id),
        move || warning_group.set_visible(false),
    );
}

pub(super) fn sync_recipient_group_header(
    state: &KeyManagementUiState,
    scope_selector_visible: bool,
) {
    if scope_selector_visible {
        state.widgets.recipient_keys_group.set_title("");
        state.widgets.recipient_keys_group.set_description(None);
    } else {
        state
            .widgets
            .recipient_keys_group
            .set_title(&gettext(RECIPIENT_KEYS_GROUP_TITLE));
        state
            .widgets
            .recipient_keys_group
            .set_description(Some(&gettext(RECIPIENT_KEYS_GROUP_DESCRIPTION)));
    }
}

pub(super) fn append_recipient_projection_row(state: &KeyManagementUiState, row: &ActionRow) {
    add_tracked_preferences_group_child(
        &state.widgets.recipient_keys_group,
        state.recipient_rows.as_ref(),
        row,
    );
}

fn inspect_private_key_lock_state(fingerprint: &str) -> (bool, bool) {
    let unlocked = match is_ripasso_private_key_unlocked(fingerprint) {
        Ok(unlocked) => unlocked,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' is unlocked: {err}"
            ));
            false
        }
    };
    let requires_unlock = match ripasso_private_key_requires_session_unlock(fingerprint) {
        Ok(requires_unlock) => requires_unlock,
        Err(err) => {
            log_error(format!(
                "Failed to inspect whether private key '{fingerprint}' requires unlocking: {err}"
            ));
            false
        }
    };

    (unlocked, requires_unlock)
}

fn recipient_matches_key(
    recipient: &str,
    key: &AvailableRecipientKey,
    matcher: &RecipientKeyMatcher,
) -> bool {
    matcher(recipient, key.fingerprint(), key.user_ids())
}

fn selected_key_count(
    recipients: &[String],
    keys: &[AvailableRecipientKey],
    matcher: &RecipientKeyMatcher,
) -> usize {
    keys.iter()
        .filter(|key| {
            recipients
                .iter()
                .any(|recipient| recipient_matches_key(recipient, key, matcher))
        })
        .count()
}

fn key_is_currently_usable(key: &AvailableRecipientKey) -> bool {
    match key {
        AvailableRecipientKey::Managed(key) => {
            let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
            unlocked || !requires_unlock
        }
        AvailableRecipientKey::ConnectedSmartcard(key) => {
            let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
            unlocked || !requires_unlock
        }
        AvailableRecipientKey::HostOnly(_) => true,
    }
}

fn selected_usable_key_count(
    recipients: &[String],
    keys: &[AvailableRecipientKey],
    matcher: &RecipientKeyMatcher,
) -> usize {
    keys.iter()
        .filter(|key| {
            recipients
                .iter()
                .any(|recipient| recipient_matches_key(recipient, key, matcher))
                && key_is_currently_usable(key)
        })
        .count()
}

fn unresolved_recipients(
    recipients: &[String],
    keys: &[AvailableRecipientKey],
    matcher: &RecipientKeyMatcher,
) -> Vec<String> {
    let mut unresolved = Vec::new();
    for recipient in recipients {
        if keys
            .iter()
            .any(|key| recipient_matches_key(recipient, key, matcher))
            || unresolved.iter().any(|existing| existing == recipient)
        {
            continue;
        }
        unresolved.push(recipient.clone());
    }
    unresolved
}

fn merge_available_recipient_keys(
    managed_keys: Vec<ManagedRipassoPrivateKey>,
    connected_smartcards: Vec<ConnectedSmartcardKey>,
    host_keys: Vec<HostGpgPrivateKeySummary>,
) -> Vec<AvailableRecipientKey> {
    let mut seen_fingerprints: HashSet<String> = managed_keys
        .iter()
        .map(|key| key.fingerprint.to_ascii_lowercase())
        .collect();
    let mut keys = managed_keys
        .into_iter()
        .map(AvailableRecipientKey::Managed)
        .collect::<Vec<_>>();

    for key in connected_smartcards {
        if seen_fingerprints.insert(key.fingerprint.to_ascii_lowercase()) {
            keys.push(AvailableRecipientKey::ConnectedSmartcard(key));
        }
    }
    for key in host_keys {
        if seen_fingerprints.insert(key.fingerprint.to_ascii_lowercase()) {
            keys.push(AvailableRecipientKey::HostOnly(key));
        }
    }

    keys.sort_by(|left, right| {
        left.title()
            .to_ascii_lowercase()
            .cmp(&right.title().to_ascii_lowercase())
            .then_with(|| left.fingerprint().cmp(right.fingerprint()))
    });
    keys
}

fn load_available_recipient_keys(
    state: &KeyManagementUiState,
    managed_keys: Vec<ManagedRipassoPrivateKey>,
    connected_smartcards: Vec<ConnectedSmartcardKey>,
    uses_host_backend: bool,
) -> (Vec<AvailableRecipientKey>, bool) {
    if !uses_host_backend {
        return (
            merge_available_recipient_keys(managed_keys, connected_smartcards, Vec::new()),
            false,
        );
    }

    match (state.ports.list_host_private_keys)() {
        Ok(host_keys) => (
            merge_available_recipient_keys(managed_keys, connected_smartcards, host_keys),
            false,
        ),
        Err(err) => {
            log_error(format!(
                "Failed to inspect host GPG private keys for recipients: {err}"
            ));
            (
                merge_available_recipient_keys(managed_keys, connected_smartcards, Vec::new()),
                true,
            )
        }
    }
}

fn verification_warning(
    uses_host_backend: bool,
    sync_enabled: bool,
    host_key_inspection_failed: bool,
) -> Option<RecipientKeyVerificationWarning> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    if uses_host_backend && host_key_inspection_failed {
        Some(RecipientKeyVerificationWarning::HostInspectionFailed)
    } else if !uses_host_backend && !sync_enabled {
        Some(RecipientKeyVerificationWarning::SyncDisabled)
    } else {
        None
    }
}

fn sync_verification_warning(
    state: &KeyManagementUiState,
    warning: Option<RecipientKeyVerificationWarning>,
) {
    let show_warning =
        warning.is_some() && !(state.ports.is_notice_hidden)(HOST_GPG_WARNING_NOTICE_ID);
    if let Some(warning) = warning {
        state
            .widgets
            .recipient_host_gpg_warning_row
            .set_title(&gettext(warning.title()));
        state
            .widgets
            .recipient_host_gpg_warning_row
            .set_subtitle(&gettext(warning.subtitle()));
    }
    state
        .widgets
        .recipient_host_gpg_warning_group
        .set_visible(show_warning);
}

fn sync_delete_button(button: &adw::gtk::Button, blocked_message: Option<&str>) {
    button.set_sensitive(blocked_message.is_none());
    button.set_tooltip_text(Some(&gettext(blocked_message.unwrap_or("Remove key file"))));
}

fn sync_toggle_button(button: &adw::gtk::CheckButton, blocked_message: Option<&str>) {
    button.set_sensitive(blocked_message.is_none());
    let tooltip = blocked_message.map(gettext);
    button.set_tooltip_text(tooltip.as_deref());
}

fn append_key_row_shell(
    title: &str,
    subtitle: &str,
    active: bool,
    toggle_blocked_message: Option<&str>,
) -> (ActionRow, adw::gtk::CheckButton) {
    let escaped_title = adw::glib::markup_escape_text(title);
    let row = ActionRow::builder()
        .title(escaped_title.as_str())
        .subtitle(subtitle)
        .build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon("dialog-password-symbolic"));
    let toggle = adw::gtk::CheckButton::new();
    toggle.set_active(active);
    sync_toggle_button(&toggle, toggle_blocked_message);
    row.add_suffix(&toggle);
    (row, toggle)
}

fn key_button_group() -> adw::gtk::Box {
    let group = adw::gtk::Box::new(adw::gtk::Orientation::Horizontal, 0);
    group.set_valign(adw::gtk::Align::Center);
    group.add_css_class("linked");
    group
}

fn unlock_button(
    state: &KeyManagementUiState,
    fingerprint: &str,
    unlocked: bool,
    requires_unlock: bool,
    enabled: bool,
) -> Option<adw::gtk::Button> {
    if !enabled || unlocked || !requires_unlock {
        return None;
    }

    let button = flat_icon_button_with_tooltip("changes-prevent-symbolic", "Unlock key");
    let state = state.clone();
    let fingerprint = fingerprint.to_string();
    let finish_button = button.clone();
    button.connect_clicked(move |_| {
        finish_button.set_sensitive(false);
        let state_for_success = state.clone();
        let after_unlock: Rc<dyn Fn()> =
            Rc::new(move || state_for_success.notify_key_access_changed());
        let on_finish: Rc<dyn Fn(bool)> = Rc::new({
            let finish_button = finish_button.clone();
            move |success| {
                if !success {
                    finish_button.set_sensitive(true);
                }
            }
        });
        (state.ports.prompt_unlock)(&state.overlay, fingerprint.clone(), after_unlock, on_finish);
    });
    Some(button)
}

fn connect_toggle(
    toggle: &adw::gtk::CheckButton,
    key: &AvailableRecipientKey,
    policy: &RecipientKeyListPolicy,
) {
    let fingerprint = key.fingerprint().to_string();
    let user_ids = key.user_ids().to_vec();
    let on_toggle = policy.on_toggle.clone();
    toggle.connect_toggled(move |button| {
        on_toggle(fingerprint.clone(), user_ids.clone(), button.is_active());
    });
}

fn copy_text(state: &KeyManagementUiState, text: &str, button: &adw::gtk::Button) {
    let _ = (state.ports.copy_text)(text, &state.overlay, Some(button));
}

fn append_managed_key_row(
    state: &KeyManagementUiState,
    key: &ManagedRipassoPrivateKey,
    active: bool,
    selected_keys: usize,
    selected_usable_keys: usize,
    context: &RecipientKeyListContext,
) {
    let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
    let usable = unlocked || !requires_unlock;
    let toggle_message = (context.policy.toggle_blocked_message)(
        active,
        usable,
        selected_keys,
        selected_usable_keys,
    );
    let delete_message = (context.policy.delete_blocked_message)(active, selected_keys);
    let (row, toggle) = append_key_row_shell(
        &key.title(),
        &super::super::managed_key_subtitle(key),
        active,
        toggle_message.as_deref(),
    );
    let copy_button = flat_icon_button_with_tooltip(
        "edit-copy-symbolic",
        super::super::managed_key_copy_tooltip(key),
    );
    let delete_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove key");
    sync_delete_button(&delete_button, delete_message.as_deref());
    let buttons = key_button_group();
    if let Some(button) = unlock_button(
        state,
        &key.fingerprint,
        unlocked,
        requires_unlock,
        context.uses_integrated_backend,
    ) {
        buttons.append(&button);
    }
    buttons.append(&copy_button);
    buttons.append(&delete_button);
    row.add_suffix(&buttons);
    append_recipient_projection_row(state, &row);
    connect_toggle(
        &toggle,
        &AvailableRecipientKey::Managed(key.clone()),
        &context.policy,
    );

    let state_for_copy = state.clone();
    let key_for_copy = key.clone();
    let copy_button_for_click = copy_button.clone();
    copy_button.connect_clicked(move |_| match armored_managed_key_material(&key_for_copy) {
        Ok(armored) => copy_text(&state_for_copy, &armored, &copy_button_for_click),
        Err(err) => {
            log_error(format!(
                "Failed to copy key material '{}': {err}",
                key_for_copy.fingerprint
            ));
            state_for_copy
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't copy that key.")));
        }
    });

    let state_for_delete = state.clone();
    let fingerprint = key.fingerprint.clone();
    delete_button.connect_clicked(move |_| {
        if let Err(err) = remove_ripasso_private_key(&fingerprint) {
            log_error(format!(
                "Failed to remove private key '{fingerprint}': {err}"
            ));
            state_for_delete
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't remove that key.")));
            return;
        }
        state_for_delete.notify_key_changed();
        state_for_delete
            .overlay
            .add_toast(Toast::new(&gettext("Key file removed.")));
    });
}

fn append_host_key_row(
    state: &KeyManagementUiState,
    key: &HostGpgPrivateKeySummary,
    active: bool,
    selected_keys: usize,
    selected_usable_keys: usize,
    context: &RecipientKeyListContext,
) {
    let toggle_message =
        (context.policy.toggle_blocked_message)(active, true, selected_keys, selected_usable_keys);
    let (row, toggle) = append_key_row_shell(
        &key.title(),
        &key.fingerprint,
        active,
        toggle_message.as_deref(),
    );
    let copy_button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy fingerprint");
    let buttons = key_button_group();
    buttons.append(&copy_button);
    row.add_suffix(&buttons);
    append_recipient_projection_row(state, &row);
    connect_toggle(
        &toggle,
        &AvailableRecipientKey::HostOnly(key.clone()),
        &context.policy,
    );
    let state = state.clone();
    let fingerprint = key.fingerprint.clone();
    let button = copy_button.clone();
    copy_button.connect_clicked(move |_| copy_text(&state, &fingerprint, &button));
}

fn connected_smartcard_subtitle(key: &ConnectedSmartcardKey) -> String {
    let detail = key
        .hardware
        .reader_hint
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| key.hardware.ident.clone());
    if detail.trim().is_empty() {
        gettext("{fingerprint} - Security token").replace("{fingerprint}", &key.fingerprint)
    } else {
        gettext("{fingerprint} - Security token ({detail})")
            .replace("{fingerprint}", &key.fingerprint)
            .replace("{detail}", &detail)
    }
}

fn append_smartcard_key_row(
    state: &KeyManagementUiState,
    key: &ConnectedSmartcardKey,
    active: bool,
    selected_keys: usize,
    selected_usable_keys: usize,
    context: &RecipientKeyListContext,
) {
    let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
    let toggle_message = (context.policy.toggle_blocked_message)(
        active,
        unlocked || !requires_unlock,
        selected_keys,
        selected_usable_keys,
    );
    let (row, toggle) = append_key_row_shell(
        &key.title(),
        &connected_smartcard_subtitle(key),
        active,
        toggle_message.as_deref(),
    );
    let copy_button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy fingerprint");
    let buttons = key_button_group();
    if let Some(button) = unlock_button(
        state,
        &key.fingerprint,
        unlocked,
        requires_unlock,
        context.uses_integrated_backend,
    ) {
        buttons.append(&button);
    }
    buttons.append(&copy_button);
    row.add_suffix(&buttons);
    append_recipient_projection_row(state, &row);
    connect_toggle(
        &toggle,
        &AvailableRecipientKey::ConnectedSmartcard(key.clone()),
        &context.policy,
    );
    let state = state.clone();
    let fingerprint = key.fingerprint.clone();
    let button = copy_button.clone();
    copy_button.connect_clicked(move |_| copy_text(&state, &fingerprint, &button));
}

pub(super) fn rebuild_recipient_key_list(
    state: &KeyManagementUiState,
    context: RecipientKeyListContext,
) {
    clear_tracked_preferences_group(
        &state.widgets.recipient_keys_group,
        state.recipient_rows.as_ref(),
    );
    sync_verification_warning(state, None);

    let managed_keys = match list_ripasso_private_keys() {
        Ok(keys) => keys,
        Err(err) => {
            log_error(format!("Failed to load private keys for recipients: {err}"));
            (context.policy.before_key_rows)(false, Vec::new());
            let row = append_info_group_row(
                &state.widgets.recipient_keys_group,
                "Couldn't load private keys",
                "Try again from Preferences.",
            );
            state.recipient_rows.borrow_mut().push(row.upcast());
            return;
        }
    };
    let connected_smartcards = if context.uses_integrated_backend {
        match list_connected_smartcard_keys() {
            Ok(keys) => keys,
            Err(err) => {
                log_error(format!(
                    "Failed to inspect connected smartcards for recipients: {err}"
                ));
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let (keys, host_inspection_failed) = load_available_recipient_keys(
        state,
        managed_keys,
        connected_smartcards,
        context.uses_host_backend,
    );
    let unresolved = unresolved_recipients(
        &context.current_recipients,
        &keys,
        &context.policy.recipient_matches,
    );
    (context.policy.before_key_rows)(!keys.is_empty(), unresolved.clone());
    sync_verification_warning(
        state,
        verification_warning(
            context.uses_host_backend,
            (state.ports.private_key_sync_enabled)(),
            host_inspection_failed,
        ),
    );

    if keys.is_empty() {
        if unresolved.is_empty() {
            let row = append_info_group_row(
                &state.widgets.recipient_keys_group,
                "No recipients yet",
                "Generate or import a private key.",
            );
            state.recipient_rows.borrow_mut().push(row.upcast());
        }
        return;
    }

    let selected_keys = selected_key_count(
        &context.current_recipients,
        &keys,
        &context.policy.recipient_matches,
    );
    let selected_usable_keys = selected_usable_key_count(
        &context.current_recipients,
        &keys,
        &context.policy.recipient_matches,
    );
    for key in keys {
        let active = context.current_recipients.iter().any(|recipient| {
            recipient_matches_key(recipient, &key, &context.policy.recipient_matches)
        });
        if !(context.policy.show_choice)(active) {
            continue;
        }
        match key {
            AvailableRecipientKey::Managed(key) => append_managed_key_row(
                state,
                &key,
                active,
                selected_keys,
                selected_usable_keys,
                &context,
            ),
            AvailableRecipientKey::ConnectedSmartcard(key) => append_smartcard_key_row(
                state,
                &key,
                active,
                selected_keys,
                selected_usable_keys,
                &context,
            ),
            AvailableRecipientKey::HostOnly(key) => append_host_key_row(
                state,
                &key,
                active,
                selected_keys,
                selected_usable_keys,
                &context,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ManagedRipassoHardwareKey, ManagedRipassoPrivateKeyProtection};

    fn password_key(fingerprint: &str, user_id: &str) -> ManagedRipassoPrivateKey {
        ManagedRipassoPrivateKey {
            fingerprint: fingerprint.to_string(),
            user_ids: vec![user_id.to_string()],
            protection: ManagedRipassoPrivateKeyProtection::Password,
            hardware: None,
        }
    }

    fn matcher() -> RecipientKeyMatcher {
        Rc::new(|recipient, fingerprint, user_ids| {
            recipient.eq_ignore_ascii_case(fingerprint)
                || user_ids
                    .iter()
                    .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
        })
    }

    #[test]
    fn merged_keys_prefer_managed_duplicates() {
        let managed = password_key(
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "Managed User <managed@example.com>",
        );
        let connected = ConnectedSmartcardKey {
            fingerprint: managed.fingerprint.clone(),
            user_ids: vec!["Connected User <token@example.com>".to_string()],
            hardware: ManagedRipassoHardwareKey {
                ident: "token-a".to_string(),
                signing_fingerprint: None,
                decryption_fingerprint: None,
                reader_hint: Some("Reader A".to_string()),
            },
        };
        let merged = merge_available_recipient_keys(
            vec![managed.clone()],
            vec![connected],
            vec![HostGpgPrivateKeySummary {
                fingerprint: managed.fingerprint.clone(),
                user_ids: vec!["Host Duplicate <host@example.com>".to_string()],
            }],
        );
        assert_eq!(merged.len(), 1);
        assert!(matches!(&merged[0], AvailableRecipientKey::Managed(found) if found == &managed));
    }

    #[test]
    fn unresolved_recipients_consider_connected_keys() {
        let unresolved = unresolved_recipients(
            &[
                "Token User <token@example.com>".to_string(),
                "missing@example.com".to_string(),
            ],
            &[AvailableRecipientKey::ConnectedSmartcard(
                ConnectedSmartcardKey {
                    fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                    user_ids: vec!["Token User <token@example.com>".to_string()],
                    hardware: ManagedRipassoHardwareKey {
                        ident: "token-a".to_string(),
                        signing_fingerprint: None,
                        decryption_fingerprint: None,
                        reader_hint: Some("Reader A".to_string()),
                    },
                },
            )],
            &matcher(),
        );
        assert_eq!(unresolved, vec!["missing@example.com".to_string()]);
    }

    #[test]
    fn selected_count_only_tracks_matching_keys() {
        let keys = vec![
            AvailableRecipientKey::Managed(password_key(
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "Alice <alice@example.com>",
            )),
            AvailableRecipientKey::HostOnly(HostGpgPrivateKeySummary {
                fingerprint: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
                user_ids: vec!["Bob <bob@example.com>".to_string()],
            }),
        ];
        assert_eq!(
            selected_key_count(
                &[
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                    "Bob <bob@example.com>".to_string(),
                    "missing@example.com".to_string(),
                ],
                &keys,
                &matcher(),
            ),
            2
        );
    }

    #[test]
    fn verification_warning_matches_backend_and_sync_state() {
        #[cfg(target_os = "linux")]
        {
            assert_eq!(
                verification_warning(true, false, true),
                Some(RecipientKeyVerificationWarning::HostInspectionFailed)
            );
            assert_eq!(
                verification_warning(false, false, false),
                Some(RecipientKeyVerificationWarning::SyncDisabled)
            );
        }
        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(verification_warning(true, false, true), None);
            assert_eq!(verification_warning(false, false, false), None);
        }
    }
}
