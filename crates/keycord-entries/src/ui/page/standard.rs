use super::{open_password_entry_page, PasswordPageState};
use crate::model::OpenPassFile;
use crate::PasswordEntryError;
use adw::Toast;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;

const fn should_switch_to_integrated_backend(
    uses_integrated_backend: bool,
    error: &PasswordEntryError,
) -> bool {
    !uses_integrated_backend && !matches!(error, PasswordEntryError::EntryNotFound(_))
}

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    error: &PasswordEntryError,
) -> bool {
    if !should_switch_to_integrated_backend(
        (state.ports.preferences.uses_integrated_backend)(),
        error,
    ) {
        return false;
    }

    if let Err(err) = (state.ports.preferences.switch_to_integrated_backend)() {
        log_error(format!("Failed to switch to the integrated backend: {err}"));
        return false;
    }

    state
        .overlay
        .add_toast(Toast::new(&gettext("Using Integrated instead.")));
    open_password_entry_page(state, pass_file.clone(), false);
    true
}

#[cfg(test)]
mod tests {
    use super::should_switch_to_integrated_backend;
    use crate::PasswordEntryError;

    #[test]
    fn only_non_integrated_backends_retry_with_integrated_mode() {
        assert!(should_switch_to_integrated_backend(
            false,
            &PasswordEntryError::other("failure")
        ));
        assert!(!should_switch_to_integrated_backend(
            true,
            &PasswordEntryError::other("failure")
        ));
    }

    #[test]
    fn missing_entries_do_not_trigger_a_backend_switch() {
        assert!(!should_switch_to_integrated_backend(
            false,
            &PasswordEntryError::EntryNotFound("item was not found".to_string())
        ));
    }
}
