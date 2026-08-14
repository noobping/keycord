use super::state::show_password_status_message;
use super::{
    open_password_entry_page, password_unlock_status_text,
    save_current_password_entry_without_git_unlock_prompt, standard, PasswordPageState,
};
use crate::model::OpenPassFile;
use crate::PasswordEntryError;
use keycord_runtime::log_error;
use keycord_shell::actions::activate_widget_action;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OpenPasswordErrorAction {
    PromptUnlock,
    OpenPreferences,
    None,
}

const fn open_password_error_action(error: &PasswordEntryError) -> OpenPasswordErrorAction {
    if matches!(error, PasswordEntryError::LockedPrivateKey(_)) {
        return OpenPasswordErrorAction::PromptUnlock;
    }

    if matches!(error, PasswordEntryError::MissingPrivateKey(_)) {
        return OpenPasswordErrorAction::OpenPreferences;
    }

    OpenPasswordErrorAction::None
}

pub(super) fn handle_open_password_entry_error(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
    error: &PasswordEntryError,
) -> bool {
    if !(state.ports.preferences.uses_integrated_backend)() {
        return standard::handle_open_password_entry_error(state, pass_file, error);
    }

    if open_password_error_action(error) == OpenPasswordErrorAction::PromptUnlock {
        let (status_title, status_description) = password_unlock_status_text();
        show_password_status_message(state, status_title, status_description);
        match (state.ports.keys.preferred_fingerprint)(
            pass_file.store_path().to_string(),
            pass_file.label(),
        ) {
            Ok(fingerprint) => {
                let retry_pass_file = pass_file.clone();
                let retry_page_state = state.clone();
                (state.ports.keys.prompt_unlock)(
                    &state.overlay,
                    fingerprint,
                    Rc::new(move || {
                        open_password_entry_page(&retry_page_state, retry_pass_file.clone(), false);
                    }),
                    Rc::new({
                        let retry_page_state = state.clone();
                        move |success| {
                            if !success {
                                activate_widget_action(&retry_page_state.nav, "win.go-home");
                            }
                        }
                    }),
                );
                return true;
            }
            Err(err) => {
                log_error(format!(
                    "Failed to resolve the private key for this item: {err}"
                ));
            }
        }
    }

    if open_password_error_action(error) == OpenPasswordErrorAction::OpenPreferences {
        (state.ports.open_preferences)(&state.nav);
    }

    false
}

pub(super) fn prompt_unlock_for_git_commit_if_needed(
    state: &PasswordPageState,
    pass_file: &OpenPassFile,
) -> bool {
    if !(state.ports.preferences.uses_integrated_backend)() {
        return false;
    }

    let retry_state = state.clone();
    let after_unlock: Rc<dyn Fn()> =
        Rc::new(move || save_current_password_entry_without_git_unlock_prompt(&retry_state));
    (state.ports.prompt_git_unlock)(
        &state.overlay,
        pass_file.store_path().to_string(),
        pass_file.label(),
        after_unlock,
    )
}

#[cfg(test)]
mod tests {
    use super::{open_password_error_action, OpenPasswordErrorAction};
    use crate::PasswordEntryError;

    #[test]
    fn open_password_error_action_matches_supported_private_key_flows() {
        assert_eq!(
            open_password_error_action(&PasswordEntryError::locked_private_key("locked")),
            OpenPasswordErrorAction::PromptUnlock
        );
        assert_eq!(
            open_password_error_action(&PasswordEntryError::missing_private_key("missing")),
            OpenPasswordErrorAction::OpenPreferences
        );
    }

    #[test]
    fn open_password_error_action_ignores_other_failures() {
        assert_eq!(
            open_password_error_action(&PasswordEntryError::incompatible_private_key(
                "incompatible"
            )),
            OpenPasswordErrorAction::None
        );
        assert_eq!(
            open_password_error_action(&PasswordEntryError::other("other")),
            OpenPasswordErrorAction::None
        );
    }
}
