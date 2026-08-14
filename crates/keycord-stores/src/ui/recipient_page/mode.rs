use super::StoreRecipientsPageState;
pub(super) use crate::recipient_page::{
    show_standard_private_key_choice, store_recipients_selection_mode, StoreRecipientsSelectionMode,
};
use keycord_runtime::i18n::gettext;

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
    state
        .key_management
        .sync_recipient_action_visibility(show_standard_rows, uses_integrated_backend);
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
