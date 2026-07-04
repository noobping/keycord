use super::StoreRecipientsPageState;
use crate::i18n::gettext;
use crate::support::runtime::{
    supports_fidokey_features, supports_hardwarekey_features, supports_smartcard_features,
};
use crate::window::host_access::{
    append_optional_fido2_access_group_row, append_optional_smartcard_access_group_row,
};
use adw::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StoreRecipientsSelectionMode {
    Empty,
    StandardOnly,
}

impl StoreRecipientsSelectionMode {
    const fn allows_standard_recipients(self) -> bool {
        matches!(self, Self::Empty | Self::StandardOnly)
    }

    const fn shows_standard_recipient_choice(self, _active: bool) -> bool {
        true
    }

    const fn standard_action_block_message(self) -> Option<&'static str> {
        None
    }
}

pub(super) fn store_recipients_selection_mode(
    recipients: &[String],
) -> StoreRecipientsSelectionMode {
    if recipients.is_empty() {
        StoreRecipientsSelectionMode::Empty
    } else {
        StoreRecipientsSelectionMode::StandardOnly
    }
}

pub(super) fn current_selection_mode(
    state: &StoreRecipientsPageState,
) -> StoreRecipientsSelectionMode {
    store_recipients_selection_mode(&state.recipients.borrow())
}

pub(super) fn sync_store_recipients_mode_controls(
    state: &StoreRecipientsPageState,
    selection_mode: StoreRecipientsSelectionMode,
    uses_integrated_backend: bool,
) {
    let show_standard_rows = selection_mode.allows_standard_recipients();
    let smartcard_supported = supports_smartcard_features();
    let hardwarekey_supported = supports_hardwarekey_features();
    let fidokey_supported = supports_fidokey_features();
    let show_generic_import_rows = show_standard_rows;

    state
        .platform
        .generate_key_row
        .set_visible(show_standard_rows);
    state
        .platform
        .generate_fido2_key_row
        .set_visible(show_standard_rows && fidokey_supported);
    state
        .platform
        .import_clipboard_row
        .set_visible(show_generic_import_rows);
    state
        .platform
        .import_file_row
        .set_visible(show_generic_import_rows);
    state
        .platform
        .setup_hardware_key_row
        .set_visible(show_standard_rows && hardwarekey_supported);
    state
        .platform
        .add_hardware_key_row
        .set_visible(show_standard_rows && smartcard_supported);
    state
        .platform
        .import_hardware_key_row
        .set_visible(show_standard_rows && smartcard_supported);

    append_optional_smartcard_access_group_row(
        &state.platform.add_list,
        &state.platform.overlay,
        &[
            &state.platform.setup_hardware_key_row,
            &state.platform.add_hardware_key_row,
            &state.platform.import_hardware_key_row,
        ],
        show_standard_rows
            && (state.platform.setup_hardware_key_row.is_visible()
                || state.platform.add_hardware_key_row.is_visible()
                || state.platform.import_hardware_key_row.is_visible()),
    );
    append_optional_fido2_access_group_row(
        &state.platform.add_list,
        &state.platform.overlay,
        &[&state.platform.generate_fido2_key_row],
        uses_integrated_backend && state.platform.generate_fido2_key_row.is_visible(),
    );

    state
        .platform
        .create_group
        .set_visible(state.platform.generate_key_row.is_visible());
    state.platform.add_group.set_visible(
        state.platform.generate_fido2_key_row.is_visible()
            || state.platform.setup_hardware_key_row.is_visible()
            || state.platform.add_hardware_key_row.is_visible()
            || state.platform.import_hardware_key_row.is_visible()
            || state.platform.import_clipboard_row.is_visible()
            || state.platform.import_file_row.is_visible(),
    );
}

pub(super) fn show_standard_private_key_choice(
    selection_mode: StoreRecipientsSelectionMode,
    active: bool,
) -> bool {
    selection_mode.shows_standard_recipient_choice(active)
}

fn toast_blocked_action(state: &StoreRecipientsPageState, message: Option<&'static str>) -> bool {
    let Some(message) = message else {
        return true;
    };

    state
        .platform
        .overlay
        .add_toast(adw::Toast::new(&gettext(message)));
    false
}

pub(super) fn ensure_standard_recipient_actions_allowed(state: &StoreRecipientsPageState) -> bool {
    let selection_mode = current_selection_mode(state);
    toast_blocked_action(state, selection_mode.standard_action_block_message())
}

#[cfg(test)]
mod tests {
    use super::{
        show_standard_private_key_choice, store_recipients_selection_mode,
        StoreRecipientsSelectionMode,
    };

    #[test]
    fn recipients_selection_mode_tracks_empty_and_standard_stores() {
        assert_eq!(
            store_recipients_selection_mode(&[]),
            StoreRecipientsSelectionMode::Empty
        );
        assert_eq!(
            store_recipients_selection_mode(&["alice@example.com".to_string()]),
            StoreRecipientsSelectionMode::StandardOnly
        );
    }

    #[test]
    fn standard_key_choices_are_available() {
        assert!(show_standard_private_key_choice(
            StoreRecipientsSelectionMode::Empty,
            false
        ));
        assert!(show_standard_private_key_choice(
            StoreRecipientsSelectionMode::StandardOnly,
            false
        ));
        assert!(show_standard_private_key_choice(
            StoreRecipientsSelectionMode::StandardOnly,
            true
        ));
    }

    #[test]
    fn standard_actions_are_allowed() {
        assert_eq!(
            StoreRecipientsSelectionMode::Empty.standard_action_block_message(),
            None
        );
        assert_eq!(
            StoreRecipientsSelectionMode::StandardOnly.standard_action_block_message(),
            None
        );
    }
}
