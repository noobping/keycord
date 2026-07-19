use super::export::copy_managed_key_material;
use super::mode::{
    current_selection_mode, show_standard_private_key_choice, sync_store_recipients_mode_controls,
    StoreRecipientsSelectionMode,
};
use super::sync::{sync_private_keys_from_host_if_enabled, sync_private_keys_to_host_if_enabled};
use super::{
    load_store_recipients_scope, queue_store_recipients_autosave, StoreRecipientsPageState,
};
use crate::backend::{
    is_ripasso_private_key_unlocked, list_connected_smartcard_keys, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_session_unlock, ConnectedSmartcardKey,
    ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection,
    StoreRecipientsPrivateKeyRequirement,
};
#[cfg(target_os = "linux")]
use crate::backend::{list_host_gpg_private_keys, HostGpgPrivateKeySummary};
use crate::clipboard::set_clipboard_text;
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::preferences::Preferences;
use crate::private_key::unlock::prompt_private_key_unlock_for_action;
use crate::store::git_page::rebuild_store_recipients_git_row;
use crate::store::recipients::{relevant_store_recipient_scopes, ROOT_STORE_RECIPIENTS_SCOPE};
use crate::support::actions::activate_widget_action;
use crate::support::ui::{
    add_persistent_hide_button, add_tracked_preferences_group_child, append_info_group_row,
    clear_tracked_preferences_group, dim_label_icon, flat_icon_button_with_tooltip,
};
use adw::gtk::StringList;
use adw::prelude::*;
use adw::{ActionRow, Toast};
use std::collections::HashSet;
use std::rc::Rc;

#[cfg(not(target_os = "linux"))]
#[derive(Clone, Debug, PartialEq, Eq)]
struct HostGpgPrivateKeySummary {
    fingerprint: String,
    user_ids: Vec<String>,
}

#[cfg(not(target_os = "linux"))]
impl HostGpgPrivateKeySummary {
    fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| gettext("Unnamed host private key"))
    }
}

#[cfg(not(target_os = "linux"))]
fn list_host_gpg_private_keys() -> Result<Vec<HostGpgPrivateKeySummary>, String> {
    Ok(Vec::new())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum AvailablePrivateKey {
    Managed(ManagedRipassoPrivateKey),
    ConnectedSmartcard(ConnectedSmartcardKey),
    HostOnly(HostGpgPrivateKeySummary),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrivateKeyVerificationWarning {
    HostInspectionFailed,
    SyncDisabled,
}

const HOST_GPG_WARNING_NOTICE_ID: &str = "store-recipients-host-gpg-warning";
const STORE_RECIPIENTS_KEYS_GROUP_TITLE: &str = "Keys for this store";
const STORE_RECIPIENTS_KEYS_GROUP_DESCRIPTION: &str =
    "Select the private keys that can unlock passwords in this store.";

impl PrivateKeyVerificationWarning {
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

impl AvailablePrivateKey {
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

fn recipient_matches_parts(recipient: &str, fingerprint: &str, user_ids: &[String]) -> bool {
    let recipient = recipient.trim();
    recipient.eq_ignore_ascii_case(fingerprint)
        || user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

pub(super) fn recipient_matches_private_key(
    recipient: &str,
    key: &ManagedRipassoPrivateKey,
) -> bool {
    recipient_matches_parts(recipient, &key.fingerprint, &key.user_ids)
}

fn recipient_matches_available_private_key(recipient: &str, key: &AvailablePrivateKey) -> bool {
    recipient_matches_parts(recipient, key.fingerprint(), key.user_ids())
}

fn set_private_key_recipient_enabled(
    state: &StoreRecipientsPageState,
    fingerprint: &str,
    user_ids: &[String],
    enabled: bool,
) -> bool {
    set_private_key_recipient_values(
        &mut state.recipients.borrow_mut(),
        fingerprint,
        user_ids,
        enabled,
    )
}

fn set_private_key_requirement(
    state: &StoreRecipientsPageState,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> bool {
    let changed = state.private_key_requirement.get() != private_key_requirement;
    if changed {
        state.private_key_requirement.set(private_key_requirement);
    }
    changed
}

fn set_private_key_recipient_values(
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

fn selected_available_private_key_count(
    recipients: &[String],
    keys: &[AvailablePrivateKey],
) -> usize {
    keys.iter()
        .filter(|key| {
            recipients
                .iter()
                .any(|recipient| recipient_matches_available_private_key(recipient, key))
        })
        .count()
}

fn private_key_is_currently_usable(key: &AvailablePrivateKey) -> bool {
    match key {
        AvailablePrivateKey::Managed(key) => {
            let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
            unlocked || !requires_unlock
        }
        AvailablePrivateKey::ConnectedSmartcard(key) => {
            let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
            unlocked || !requires_unlock
        }
        AvailablePrivateKey::HostOnly(_) => true,
    }
}

fn selected_usable_private_key_count(recipients: &[String], keys: &[AvailablePrivateKey]) -> usize {
    keys.iter()
        .filter(|key| {
            recipients
                .iter()
                .any(|recipient| recipient_matches_available_private_key(recipient, key))
                && private_key_is_currently_usable(key)
        })
        .count()
}

fn private_key_delete_block_message(
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

fn private_key_toggle_block_message(
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

fn sync_private_key_delete_button(delete_button: &adw::gtk::Button, blocked_message: Option<&str>) {
    delete_button.set_sensitive(blocked_message.is_none());
    let tooltip = gettext(blocked_message.unwrap_or("Remove key file"));
    delete_button.set_tooltip_text(Some(&tooltip));
}

fn sync_private_key_toggle_button(toggle: &adw::gtk::CheckButton, blocked_message: Option<&str>) {
    toggle.set_sensitive(blocked_message.is_none());
    let tooltip = blocked_message.map(gettext);
    toggle.set_tooltip_text(tooltip.as_deref());
}

fn unresolved_private_key_recipients(
    recipients: &[String],
    keys: &[AvailablePrivateKey],
) -> Vec<String> {
    let mut unresolved = Vec::new();

    for recipient in recipients {
        if keys
            .iter()
            .any(|key| recipient_matches_available_private_key(recipient, key))
        {
            continue;
        }
        if unresolved.iter().any(|existing| existing == recipient) {
            continue;
        }
        unresolved.push(recipient.clone());
    }

    unresolved
}

fn append_unresolved_private_key_rows(state: &StoreRecipientsPageState, recipients: &[String]) {
    if recipients.is_empty() {
        return;
    }

    for recipient in recipients {
        let row = ActionRow::builder()
            .title(recipient)
            .subtitle(gettext("This recipient is not available in the app."))
            .build();
        row.set_activatable(false);
        row.add_prefix(&dim_label_icon("dialog-warning-symbolic"));

        let delete_button =
            flat_icon_button_with_tooltip("user-trash-symbolic", "Remove recipient");
        row.add_suffix(&delete_button);
        add_tracked_preferences_group_child(&state.list, state.key_rows.as_ref(), &row);

        let page_state = state.clone();
        let recipient = recipient.clone();
        delete_button.connect_clicked(move |_| {
            let before = page_state.recipients.borrow().clone();
            page_state
                .recipients
                .borrow_mut()
                .retain(|value| value != &recipient);
            super::rebuild_store_recipients_list(&page_state);
            if *page_state.recipients.borrow() != before {
                queue_store_recipients_autosave(&page_state);
            }
        });
    }
}

fn available_recipient_scopes(state: &StoreRecipientsPageState) -> Vec<String> {
    let Some(request) = state.current_request() else {
        return vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()];
    };
    if !Preferences::new().uses_integrated_backend() {
        return vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()];
    }

    let scopes = relevant_store_recipient_scopes(&request.store);
    if scopes.is_empty() {
        vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()]
    } else {
        scopes
    }
}

fn show_recipient_scope_selector(scopes: &[String]) -> bool {
    scopes.len() > 1
}

fn recipient_scope_label(scope: &str) -> String {
    if scope == ROOT_STORE_RECIPIENTS_SCOPE {
        gettext("Default")
    } else {
        scope.to_string()
    }
}

fn scope_row_model(scopes: &[String]) -> StringList {
    let labels = scopes
        .iter()
        .map(|scope| recipient_scope_label(scope))
        .collect::<Vec<_>>();
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    StringList::new(&label_refs)
}

fn sync_recipient_group_headers(state: &StoreRecipientsPageState, show_scope_selector: bool) {
    state.platform.scope_group.set_visible(show_scope_selector);
    if show_scope_selector {
        state.platform.keys_group.set_title("");
        state.platform.keys_group.set_description(None);
    } else {
        state
            .platform
            .keys_group
            .set_title(&gettext(STORE_RECIPIENTS_KEYS_GROUP_TITLE));
        state
            .platform
            .keys_group
            .set_description(Some(&gettext(STORE_RECIPIENTS_KEYS_GROUP_DESCRIPTION)));
    }
}

pub(super) fn sync_store_recipients_busy_indicator(state: &StoreRecipientsPageState) {
    state
        .platform
        .saving_group
        .set_visible(state.save_in_flight.get());
}

fn sync_recipient_scope_row(state: &StoreRecipientsPageState) {
    let scopes = available_recipient_scopes(state);
    let current_scope = state.current_recipient_scope();
    let selected_scope = if scopes.iter().any(|scope| scope == &current_scope) {
        current_scope
    } else {
        scopes
            .first()
            .cloned()
            .unwrap_or_else(|| ROOT_STORE_RECIPIENTS_SCOPE.to_string())
    };
    let show_scope_selector = show_recipient_scope_selector(&scopes);
    let scopes_changed = *state.recipient_scope_dirs.borrow() != scopes;

    if scopes_changed {
        *state.recipient_scope_dirs.borrow_mut() = scopes.clone();
        let model = scope_row_model(&scopes);
        state.platform.scope_row.set_model(Some(&model));
    }
    sync_recipient_group_headers(state, show_scope_selector);
    state.platform.scope_list.set_visible(show_scope_selector);
    state.platform.scope_row.set_visible(show_scope_selector);
    state.platform.scope_row.set_sensitive(show_scope_selector);

    let selected_position = scopes
        .iter()
        .position(|scope| scope == &selected_scope)
        .unwrap_or(0);
    if state.platform.scope_row.selected() != selected_position as u32 {
        state
            .platform
            .scope_row
            .set_selected(selected_position as u32);
    }

    if selected_scope != state.current_recipient_scope() {
        let Some(request) = state.current_request() else {
            return;
        };
        load_store_recipients_scope(state, &request.store, &selected_scope);
    }
}

pub(super) fn refresh_recipient_scope_row(state: &StoreRecipientsPageState) {
    sync_recipient_scope_row(state);
}

fn show_require_all_private_keys_option(
    _selection_mode: StoreRecipientsSelectionMode,
    has_keys: bool,
) -> bool {
    has_keys
}

fn show_store_options_title_above_git_row(show_options_group: bool, show_git: bool) -> bool {
    show_git && !show_options_group
}

fn sync_private_key_requirement_row(
    state: &StoreRecipientsPageState,
    selection_mode: StoreRecipientsSelectionMode,
    has_keys: bool,
) {
    let preferences = Preferences::new();
    let uses_integrated_backend = preferences.uses_integrated_backend();
    let show_require_all = show_require_all_private_keys_option(selection_mode, has_keys);
    let show_store_options_title = show_store_options_title_above_git_row(
        show_require_all,
        state.platform.git_group.is_visible(),
    );
    let git_group_title = if show_store_options_title {
        gettext("Experimental store options")
    } else {
        String::new()
    };

    state.platform.options_group.set_visible(show_require_all);
    state.platform.git_group.set_title(&git_group_title);
    state.platform.require_all_row.set_visible(show_require_all);
    state
        .platform
        .require_all_row
        .set_sensitive(show_require_all && uses_integrated_backend);
    state
        .platform
        .require_all_check
        .set_sensitive(show_require_all && uses_integrated_backend);
    state.platform.require_all_check.set_active(matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ));
}

fn effective_private_key_verification_warning(
    _selection_mode: StoreRecipientsSelectionMode,
    warning: Option<PrivateKeyVerificationWarning>,
) -> Option<PrivateKeyVerificationWarning> {
    warning
}

fn sync_private_key_verification_warning(
    state: &StoreRecipientsPageState,
    selection_mode: StoreRecipientsSelectionMode,
    warning: Option<PrivateKeyVerificationWarning>,
) {
    let warning = effective_private_key_verification_warning(selection_mode, warning);
    let show_warning =
        warning.is_some() && !Preferences::new().is_notice_hidden(HOST_GPG_WARNING_NOTICE_ID);

    if let Some(warning) = warning {
        state
            .platform
            .host_gpg_warning_row
            .set_title(&gettext(warning.title()));
        state
            .platform
            .host_gpg_warning_row
            .set_subtitle(&gettext(warning.subtitle()));
    }
    state
        .platform
        .host_gpg_warning_group
        .set_visible(show_warning);
}

pub(super) fn connect_private_key_requirement_control(state: &StoreRecipientsPageState) {
    let row = state.platform.require_all_row.clone();
    let check = state.platform.require_all_check.clone();
    let check_for_row = check.clone();
    row.connect_activated(move |_| {
        check_for_row.set_active(!check_for_row.is_active());
    });

    let page_state = state.clone();
    check.connect_toggled(move |button| {
        let private_key_requirement = if button.is_active() {
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        } else {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey
        };
        if set_private_key_requirement(&page_state, private_key_requirement) {
            super::rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        }
    });
}

pub(super) fn connect_recipient_scope_control(state: &StoreRecipientsPageState) {
    let row = state.platform.scope_row.clone();
    let page_state = state.clone();
    row.connect_selected_notify(move |row| {
        let Some(request) = page_state.current_request() else {
            return;
        };
        let scopes = page_state.recipient_scope_dirs.borrow().clone();
        let Some(scope) = scopes.get(row.selected() as usize).cloned() else {
            return;
        };
        if scope == page_state.current_recipient_scope() {
            return;
        }

        load_store_recipients_scope(&page_state, &request.store, &scope);
        super::rebuild_store_recipients_list(&page_state);
    });
}

pub(super) fn connect_dismissible_notice_controls(state: &StoreRecipientsPageState) {
    let host_warning_group = state.platform.host_gpg_warning_group.clone();
    add_persistent_hide_button(
        &state.platform.host_gpg_warning_row,
        HOST_GPG_WARNING_NOTICE_ID,
        move || host_warning_group.set_visible(false),
    );
}

fn merge_available_private_keys(
    managed_keys: Vec<ManagedRipassoPrivateKey>,
    connected_smartcards: Vec<ConnectedSmartcardKey>,
    host_keys: Vec<HostGpgPrivateKeySummary>,
) -> Vec<AvailablePrivateKey> {
    let mut seen_fingerprints: HashSet<String> = managed_keys
        .iter()
        .map(|key| key.fingerprint.to_ascii_lowercase())
        .collect();
    let mut keys: Vec<AvailablePrivateKey> = managed_keys
        .into_iter()
        .map(AvailablePrivateKey::Managed)
        .collect();

    for key in connected_smartcards {
        if seen_fingerprints.insert(key.fingerprint.to_ascii_lowercase()) {
            keys.push(AvailablePrivateKey::ConnectedSmartcard(key));
        }
    }

    for key in host_keys {
        if seen_fingerprints.insert(key.fingerprint.to_ascii_lowercase()) {
            keys.push(AvailablePrivateKey::HostOnly(key));
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

fn private_key_verification_warning(
    uses_host_backend: bool,
    sync_enabled: bool,
    host_key_inspection_failed: bool,
) -> Option<PrivateKeyVerificationWarning> {
    if !cfg!(target_os = "linux") {
        return None;
    }

    if uses_host_backend && host_key_inspection_failed {
        Some(PrivateKeyVerificationWarning::HostInspectionFailed)
    } else if !uses_host_backend && !sync_enabled {
        Some(PrivateKeyVerificationWarning::SyncDisabled)
    } else {
        None
    }
}

fn load_available_private_keys(
    managed_keys: Vec<ManagedRipassoPrivateKey>,
    connected_smartcards: Vec<ConnectedSmartcardKey>,
    uses_host_backend: bool,
) -> (Vec<AvailablePrivateKey>, bool) {
    if !uses_host_backend {
        return (
            merge_available_private_keys(managed_keys, connected_smartcards, Vec::new()),
            false,
        );
    }

    let host_keys = list_host_gpg_private_keys();
    match host_keys {
        Ok(host_keys) => (
            merge_available_private_keys(managed_keys, connected_smartcards, host_keys),
            false,
        ),
        Err(err) => {
            log_error(format!(
                "Failed to inspect host GPG private keys for recipients: {err}"
            ));
            (
                merge_available_private_keys(managed_keys, connected_smartcards, Vec::new()),
                true,
            )
        }
    }
}

fn show_available_private_key_choice(
    selection_mode: StoreRecipientsSelectionMode,
    key: &AvailablePrivateKey,
    active: bool,
) -> bool {
    match key {
        AvailablePrivateKey::Managed(key) => match key.protection {
            ManagedRipassoPrivateKeyProtection::Password
            | ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
                show_standard_private_key_choice(selection_mode, active)
            }
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
                show_standard_private_key_choice(selection_mode, active)
            }
        },
        AvailablePrivateKey::ConnectedSmartcard(_) => {
            show_standard_private_key_choice(selection_mode, active)
        }
        AvailablePrivateKey::HostOnly(_) => {
            show_standard_private_key_choice(selection_mode, active)
        }
    }
}

pub(super) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    clear_tracked_preferences_group(&state.list, state.key_rows.as_ref());
    rebuild_store_recipients_git_row(state);
    sync_private_key_verification_warning(state, StoreRecipientsSelectionMode::Empty, None);
    let _ = sync_private_keys_from_host_if_enabled(state);
    sync_store_recipients_busy_indicator(state);
    sync_recipient_scope_row(state);
    let current_recipients = state.recipients.borrow().clone();

    let preferences = Preferences::new();
    let uses_host_backend = preferences.uses_host_command_backend();
    let uses_integrated_backend = preferences.uses_integrated_backend();
    let sync_enabled = preferences.sync_private_keys_with_host();
    let selection_mode = current_selection_mode(state);
    sync_store_recipients_mode_controls(state, selection_mode, uses_integrated_backend);

    let managed_keys = match list_ripasso_private_keys() {
        Ok(keys) => keys,
        Err(err) => {
            log_error(format!("Failed to load private keys for recipients: {err}"));
            sync_private_key_requirement_row(state, selection_mode, false);
            let row = append_info_group_row(
                &state.list,
                "Couldn't load private keys",
                "Try again from Preferences.",
            );
            state.key_rows.borrow_mut().push(row.upcast());
            return;
        }
    };

    let managed_key_count = managed_keys.len();
    let connected_smartcards = if uses_integrated_backend {
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
    let (keys, host_key_inspection_failed) =
        load_available_private_keys(managed_keys, connected_smartcards, uses_host_backend);
    let unresolved_recipients = unresolved_private_key_recipients(&current_recipients, &keys);
    let selected_available_keys = selected_available_private_key_count(&current_recipients, &keys);
    let selected_usable_keys = selected_usable_private_key_count(&current_recipients, &keys);
    sync_private_key_requirement_row(
        state,
        selection_mode,
        managed_key_count > 0 || !keys.is_empty(),
    );
    sync_private_key_verification_warning(
        state,
        selection_mode,
        private_key_verification_warning(
            uses_host_backend,
            sync_enabled,
            host_key_inspection_failed,
        ),
    );

    if keys.is_empty() {
        if unresolved_recipients.is_empty() {
            let row = append_info_group_row(
                &state.list,
                "No recipients yet",
                "Generate or import a private key.",
            );
            state.key_rows.borrow_mut().push(row.upcast());
        } else {
            append_unresolved_private_key_rows(state, &unresolved_recipients);
        }
        return;
    }

    append_unresolved_private_key_rows(state, &unresolved_recipients);
    for key in keys {
        let active = current_recipients
            .iter()
            .any(|recipient| recipient_matches_available_private_key(recipient, &key));
        if !show_available_private_key_choice(selection_mode, &key, active) {
            continue;
        }

        match key {
            AvailablePrivateKey::Managed(key) => append_managed_private_key_row(
                state,
                &key,
                selected_available_keys,
                selected_usable_keys,
            ),
            AvailablePrivateKey::ConnectedSmartcard(key) => append_connected_smartcard_row(
                state,
                &key,
                selected_available_keys,
                selected_usable_keys,
            ),
            AvailablePrivateKey::HostOnly(key) => append_host_private_key_row(
                state,
                &key,
                selected_available_keys,
                selected_usable_keys,
            ),
        }
    }
}

fn append_private_key_row_shell(
    title: &str,
    subtitle: &str,
    active: bool,
    toggle_blocked_message: Option<&str>,
) -> (ActionRow, adw::gtk::CheckButton) {
    let title = adw::glib::markup_escape_text(title);
    let row = ActionRow::builder()
        .title(title.as_str())
        .subtitle(subtitle)
        .build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon("dialog-password-symbolic"));

    let toggle = adw::gtk::CheckButton::new();
    toggle.set_active(active);
    sync_private_key_toggle_button(&toggle, toggle_blocked_message);
    row.add_suffix(&toggle);

    (row, toggle)
}

fn append_managed_private_key_row(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) {
    let active = state
        .recipients
        .borrow()
        .iter()
        .any(|recipient| recipient_matches_private_key(recipient, key));
    let require_all_selected_keys = matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    );
    let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
    let usable = unlocked || !requires_unlock;
    let toggle_blocked_message = private_key_toggle_block_message(
        active,
        usable,
        require_all_selected_keys,
        selected_available_keys,
        selected_usable_keys,
    );
    let delete_blocked_message = private_key_delete_block_message(
        active,
        require_all_selected_keys,
        selected_available_keys,
    );
    let subtitle = match key.protection {
        ManagedRipassoPrivateKeyProtection::Password => {
            gettext("{fingerprint} - Password protected").replace("{fingerprint}", &key.fingerprint)
        }
        ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
            gettext("{fingerprint} - Hardware key").replace("{fingerprint}", &key.fingerprint)
        }
        #[cfg(feature = "fidokey")]
        ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            gettext("{fingerprint} - Security key protected")
                .replace("{fingerprint}", &key.fingerprint)
        }
    };
    let (row, toggle) =
        append_private_key_row_shell(&key.title(), &subtitle, active, toggle_blocked_message);
    let copy_button = flat_icon_button_with_tooltip(
        "edit-copy-symbolic",
        match key.protection {
            ManagedRipassoPrivateKeyProtection::Password => "Copy armored private key",
            ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => "Copy armored public key",
            #[cfg(feature = "fidokey")]
            ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
                "Copy experimental FIDO2-protected private key"
            }
        },
    );
    let delete_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove key");
    sync_private_key_delete_button(&delete_button, delete_blocked_message);

    let button_group = private_key_button_group();
    if let Some(unlock_button) =
        private_key_unlock_button(state, &key.fingerprint, unlocked, requires_unlock)
    {
        button_group.append(&unlock_button);
    }
    button_group.append(&copy_button);
    button_group.append(&delete_button);
    row.add_suffix(&button_group);
    add_tracked_preferences_group_child(&state.list, state.key_rows.as_ref(), &row);

    connect_managed_private_key_row_actions(state, key, &toggle, &copy_button, &delete_button);
}

fn append_host_private_key_row(
    state: &StoreRecipientsPageState,
    key: &HostGpgPrivateKeySummary,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) {
    let active = state
        .recipients
        .borrow()
        .iter()
        .any(|recipient| recipient_matches_parts(recipient, &key.fingerprint, &key.user_ids));
    let toggle_blocked_message = private_key_toggle_block_message(
        active,
        true,
        matches!(
            state.private_key_requirement.get(),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        ),
        selected_available_keys,
        selected_usable_keys,
    );
    let (row, toggle) = append_private_key_row_shell(
        &key.title(),
        &key.fingerprint,
        active,
        toggle_blocked_message,
    );

    let copy_button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy fingerprint");
    let button_group = private_key_button_group();
    button_group.append(&copy_button);
    row.add_suffix(&button_group);
    add_tracked_preferences_group_child(&state.list, state.key_rows.as_ref(), &row);

    let state_for_toggle = state.clone();
    let fingerprint_for_toggle = key.fingerprint.clone();
    let user_ids_for_toggle = key.user_ids.clone();
    toggle.connect_toggled(move |button| {
        if set_private_key_recipient_enabled(
            &state_for_toggle,
            &fingerprint_for_toggle,
            &user_ids_for_toggle,
            button.is_active(),
        ) {
            super::rebuild_store_recipients_list(&state_for_toggle);
            queue_store_recipients_autosave(&state_for_toggle);
        }
    });

    let overlay = state.platform.overlay.clone();
    let fingerprint_for_copy = key.fingerprint.clone();
    let copy_button_for_click = copy_button.clone();
    copy_button.connect_clicked(move |_| {
        let _ = set_clipboard_text(
            &fingerprint_for_copy,
            &overlay,
            Some(&copy_button_for_click),
        );
    });
}

fn private_key_button_group() -> adw::gtk::Box {
    let group = adw::gtk::Box::new(adw::gtk::Orientation::Horizontal, 0);
    group.set_valign(adw::gtk::Align::Center);
    group.add_css_class("linked");
    group
}

fn private_key_unlock_button(
    state: &StoreRecipientsPageState,
    fingerprint: &str,
    unlocked: bool,
    requires_unlock: bool,
) -> Option<adw::gtk::Button> {
    if !Preferences::new().uses_integrated_backend() {
        return None;
    }

    if !unlocked && requires_unlock {
        let unlock_button = flat_icon_button_with_tooltip("changes-prevent-symbolic", "Unlock key");
        let state = state.clone();
        let fingerprint = fingerprint.to_string();
        let finish_button = unlock_button.clone();
        unlock_button.connect_clicked(move |_| {
            finish_button.set_sensitive(false);
            let after_unlock: Rc<dyn Fn()> = Rc::new({
                let state = state.clone();
                move || {
                    super::rebuild_store_recipients_list(&state);
                    activate_widget_action(&state.window, "win.reload-password-list");
                }
            });
            let on_finish: Rc<dyn Fn(bool)> = Rc::new({
                let finish_button = finish_button.clone();
                move |success| {
                    if !success {
                        finish_button.set_sensitive(true);
                    }
                }
            });
            prompt_private_key_unlock_for_action(
                &state.platform.overlay,
                fingerprint.clone(),
                after_unlock,
                on_finish,
            );
        });
        Some(unlock_button)
    } else {
        None
    }
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

fn connect_managed_private_key_row_actions(
    state: &StoreRecipientsPageState,
    key: &ManagedRipassoPrivateKey,
    toggle: &adw::gtk::CheckButton,
    copy_button: &adw::gtk::Button,
    delete_button: &adw::gtk::Button,
) {
    let state_for_toggle = state.clone();
    let fingerprint_for_toggle = key.fingerprint.clone();
    let user_ids_for_toggle = key.user_ids.clone();
    toggle.connect_toggled(move |button| {
        if set_private_key_recipient_enabled(
            &state_for_toggle,
            &fingerprint_for_toggle,
            &user_ids_for_toggle,
            button.is_active(),
        ) {
            super::rebuild_store_recipients_list(&state_for_toggle);
            queue_store_recipients_autosave(&state_for_toggle);
        }
    });

    let state_for_copy = state.clone();
    let key_for_copy = key.clone();
    let copy_button_for_click = copy_button.clone();
    copy_button.connect_clicked(move |_| {
        copy_managed_key_material(&state_for_copy, &key_for_copy, Some(&copy_button_for_click));
    });

    let state_for_delete = state.clone();
    let key_for_delete = key.clone();
    delete_button.connect_clicked(move |_| {
        if let Err(err) = remove_ripasso_private_key(&key_for_delete.fingerprint) {
            log_error(format!(
                "Failed to remove private key '{}': {err}",
                key_for_delete.fingerprint
            ));
            state_for_delete
                .platform
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't remove that key.")));
            return;
        }

        let _ = sync_private_keys_to_host_if_enabled(&state_for_delete);
        super::rebuild_store_recipients_list(&state_for_delete);
        activate_widget_action(&state_for_delete.window, "win.reload-password-list");
        state_for_delete
            .platform
            .overlay
            .add_toast(Toast::new(&gettext("Key file removed.")));
    });
}

fn append_connected_smartcard_row(
    state: &StoreRecipientsPageState,
    key: &ConnectedSmartcardKey,
    selected_available_keys: usize,
    selected_usable_keys: usize,
) {
    let active = state
        .recipients
        .borrow()
        .iter()
        .any(|recipient| recipient_matches_parts(recipient, &key.fingerprint, &key.user_ids));
    let (unlocked, requires_unlock) = inspect_private_key_lock_state(&key.fingerprint);
    let toggle_blocked_message = private_key_toggle_block_message(
        active,
        unlocked || !requires_unlock,
        matches!(
            state.private_key_requirement.get(),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        ),
        selected_available_keys,
        selected_usable_keys,
    );
    let (row, toggle) = append_private_key_row_shell(
        &key.title(),
        &connected_smartcard_subtitle(key),
        active,
        toggle_blocked_message,
    );
    let copy_button = flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy fingerprint");
    let button_group = private_key_button_group();
    if let Some(unlock_button) =
        private_key_unlock_button(state, &key.fingerprint, unlocked, requires_unlock)
    {
        button_group.append(&unlock_button);
    }
    button_group.append(&copy_button);
    row.add_suffix(&button_group);
    add_tracked_preferences_group_child(&state.list, state.key_rows.as_ref(), &row);

    let state_for_toggle = state.clone();
    let fingerprint_for_toggle = key.fingerprint.clone();
    let user_ids_for_toggle = key.user_ids.clone();
    toggle.connect_toggled(move |button| {
        if set_private_key_recipient_enabled(
            &state_for_toggle,
            &fingerprint_for_toggle,
            &user_ids_for_toggle,
            button.is_active(),
        ) {
            super::rebuild_store_recipients_list(&state_for_toggle);
            queue_store_recipients_autosave(&state_for_toggle);
        }
    });

    let overlay = state.platform.overlay.clone();
    let fingerprint_for_copy = key.fingerprint.clone();
    let copy_button_for_click = copy_button.clone();
    copy_button.connect_clicked(move |_| {
        let _ = set_clipboard_text(
            &fingerprint_for_copy,
            &overlay,
            Some(&copy_button_for_click),
        );
    });
}

#[cfg(test)]
mod tests {
    use super::{
        effective_private_key_verification_warning, merge_available_private_keys,
        private_key_delete_block_message, private_key_toggle_block_message,
        private_key_verification_warning, recipient_scope_label,
        selected_available_private_key_count, show_recipient_scope_selector,
        show_require_all_private_keys_option, show_store_options_title_above_git_row,
        unresolved_private_key_recipients, AvailablePrivateKey, HostGpgPrivateKeySummary,
        PrivateKeyVerificationWarning,
    };
    use crate::backend::{
        ConnectedSmartcardKey, ManagedRipassoHardwareKey, ManagedRipassoPrivateKey,
        ManagedRipassoPrivateKeyProtection,
    };
    use crate::store::recipients_page::mode::StoreRecipientsSelectionMode;

    fn password_key(fingerprint: &str, user_id: &str) -> ManagedRipassoPrivateKey {
        ManagedRipassoPrivateKey {
            fingerprint: fingerprint.to_string(),
            user_ids: vec![user_id.to_string()],
            protection: ManagedRipassoPrivateKeyProtection::Password,
            hardware: None,
        }
    }

    #[test]
    fn merged_private_keys_prefer_managed_duplicates() {
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
        let merged = merge_available_private_keys(
            vec![managed.clone()],
            vec![connected],
            vec![HostGpgPrivateKeySummary {
                fingerprint: managed.fingerprint.clone(),
                user_ids: vec!["Host Duplicate <host@example.com>".to_string()],
            }],
        );

        assert_eq!(merged.len(), 1);
        assert!(matches!(&merged[0], AvailablePrivateKey::Managed(found) if found == &managed));
    }

    #[test]
    fn unresolved_recipients_consider_available_keys() {
        let unresolved = unresolved_private_key_recipients(
            &[
                "Token User <token@example.com>".to_string(),
                "missing@example.com".to_string(),
            ],
            &[AvailablePrivateKey::ConnectedSmartcard(
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
        );

        assert_eq!(unresolved, vec!["missing@example.com".to_string()]);
    }

    #[test]
    fn private_key_verification_warning_matches_backend_sync_and_inspection_state() {
        #[cfg(target_os = "linux")]
        {
            assert_eq!(
                private_key_verification_warning(true, false, true),
                Some(PrivateKeyVerificationWarning::HostInspectionFailed)
            );
            assert_eq!(
                private_key_verification_warning(false, false, false),
                Some(PrivateKeyVerificationWarning::SyncDisabled)
            );
        }

        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(private_key_verification_warning(true, false, true), None);
            assert_eq!(private_key_verification_warning(false, false, false), None);
        }
    }

    #[test]
    fn standard_stores_keep_private_key_verification_warnings() {
        assert_eq!(
            effective_private_key_verification_warning(
                StoreRecipientsSelectionMode::StandardOnly,
                Some(PrivateKeyVerificationWarning::SyncDisabled),
            ),
            Some(PrivateKeyVerificationWarning::SyncDisabled)
        );
    }

    #[test]
    fn require_all_option_is_available_when_keys_exist() {
        assert!(!show_require_all_private_keys_option(
            StoreRecipientsSelectionMode::Empty,
            false
        ));
        assert!(show_require_all_private_keys_option(
            StoreRecipientsSelectionMode::StandardOnly,
            true
        ));
    }

    #[test]
    fn git_row_shows_store_options_title_when_it_is_the_only_option() {
        assert!(show_store_options_title_above_git_row(false, true));
        assert!(!show_store_options_title_above_git_row(true, true));
        assert!(!show_store_options_title_above_git_row(false, false));
    }

    #[test]
    fn recipient_scope_selector_only_shows_for_multiple_relevant_scopes() {
        assert!(!show_recipient_scope_selector(&[".".to_string()]));
        assert!(show_recipient_scope_selector(&[
            ".".to_string(),
            "team".to_string(),
        ]));
    }

    #[test]
    fn root_recipient_scope_uses_default_label() {
        assert_eq!(recipient_scope_label("."), "Default".to_string());
        assert_eq!(recipient_scope_label("team"), "team".to_string());
    }

    #[test]
    fn selected_available_private_key_count_only_tracks_matching_keys() {
        let keys = vec![
            AvailablePrivateKey::Managed(password_key(
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "Alice <alice@example.com>",
            )),
            AvailablePrivateKey::HostOnly(HostGpgPrivateKeySummary {
                fingerprint: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
                user_ids: vec!["Bob <bob@example.com>".to_string()],
            }),
        ];

        assert_eq!(
            selected_available_private_key_count(
                &[
                    "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
                    "Bob <bob@example.com>".to_string(),
                    "missing@example.com".to_string(),
                ],
                &keys,
            ),
            2
        );
    }

    #[cfg(feature = "fidokey")]
    #[test]
    fn selected_available_private_key_count_handles_fido2_protected_private_keys_by_fingerprint() {
        let recipient = "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string();

        assert_eq!(
            selected_available_private_key_count(
                std::slice::from_ref(&recipient),
                &[AvailablePrivateKey::Managed(ManagedRipassoPrivateKey {
                    fingerprint: recipient.clone(),
                    user_ids: vec!["Desk Key".to_string()],
                    protection: ManagedRipassoPrivateKeyProtection::Fido2HmacSecret,
                    hardware: None,
                })],
            ),
            1
        );
    }

    #[test]
    fn delete_rules_require_another_selected_key_and_disabled_require_all() {
        assert_eq!(
            private_key_delete_block_message(true, true, 2),
            Some("This selected key is required while all selected private keys are required.")
        );
        assert_eq!(
            private_key_delete_block_message(true, false, 1),
            Some("Keep another selected private key available before removing this key.")
        );
        assert_eq!(private_key_delete_block_message(true, false, 2), None);
        assert_eq!(private_key_delete_block_message(false, false, 0), None);
    }

    #[test]
    fn locked_checked_keys_only_block_unchecking_when_they_are_required() {
        assert_eq!(
            private_key_toggle_block_message(true, true, true, 2, 2),
            Some("Keep this key selected while all selected private keys are required.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, true, false, 1, 1),
            Some("Keep at least one selected private key available.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, true, false, 2, 1),
            Some("Unlock another selected private key before clearing this one.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, true, false, 2, 2),
            None
        );
        assert_eq!(
            private_key_toggle_block_message(true, false, false, 1, 0),
            Some("Keep at least one selected private key available.")
        );
        assert_eq!(
            private_key_toggle_block_message(true, false, false, 2, 0),
            None
        );
    }
}
