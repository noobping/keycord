//! Password-protected and FIDO-protected private-key UI workflows.

use super::form::{
    connect_generation_autofill_rows, connect_private_apply_visibility, validate_name_and_email,
};
use super::KeyManagementUiState;
use crate::ui::{present_private_key_password_dialog, PrivateKeyDialogHandle};
#[cfg(feature = "fido-ui")]
use crate::{generate_fido2_private_key, set_fido2_security_key_pin};
use crate::{
    generate_ripasso_private_key, import_ripasso_private_key_bytes,
    ripasso_private_key_requires_passphrase, ManagedRipassoPrivateKey, PrivateKeyError,
};
use adw::prelude::*;
use adw::Toast;
#[cfg(feature = "fido-ui")]
use keycord_fido::ui::{FidoKeyGenerationError, FidoKeyGenerationUiPorts};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::background::spawn_result_task_with_finalizer;
use keycord_shell::file_picker::choose_file_bytes;
use keycord_shell::ui::{
    build_progress_dialog, connect_row_action, push_navigation_page_if_needed,
};
use secrecy::{ExposeSecret, SecretString};
use std::rc::Rc;
#[cfg(feature = "fido-ui")]
use std::sync::Arc;

#[derive(Clone, Debug)]
struct PrivateKeyGenerationRequest {
    name: String,
    email: String,
    passphrase: SecretString,
}

impl PartialEq for PrivateKeyGenerationRequest {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.email == other.email
            && self.passphrase.expose_secret() == other.passphrase.expose_secret()
    }
}

impl Eq for PrivateKeyGenerationRequest {}

fn validate_private_key_generation_request(
    name: &str,
    email: &str,
    passphrase: &str,
    confirmation: &str,
) -> Result<PrivateKeyGenerationRequest, &'static str> {
    let (name, email) = validate_name_and_email(name, email)?;
    if passphrase.trim().is_empty() {
        return Err("Enter a key password.");
    }
    if passphrase != confirmation {
        return Err("The passwords do not match.");
    }

    Ok(PrivateKeyGenerationRequest {
        name,
        email,
        passphrase: SecretString::from(passphrase),
    })
}

fn finish_private_key_generation(
    state: &KeyManagementUiState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => {
            clear_private_key_generation_form(state);
            state.pop_generation_page_if_visible(&state.widgets.private_key_page);
            finish_generated_key(state);
        }
        Err(err) => {
            log_error(format!("Failed to generate private key: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't generate the key.")));
        }
    }
}

fn finish_generated_key(state: &KeyManagementUiState) {
    state.notify_key_changed();
    state
        .overlay
        .add_toast(Toast::new(&gettext("Key generated.")));
}

fn start_private_key_generation(
    state: &KeyManagementUiState,
    request: PrivateKeyGenerationRequest,
) {
    set_private_key_generation_loading(state, true);
    let state = state.clone();
    let state_for_finalize = state.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || {
            generate_ripasso_private_key(
                &request.name,
                &request.email,
                request.passphrase.expose_secret(),
            )
        },
        move || set_private_key_generation_loading(&state_for_finalize, false),
        move |result| finish_private_key_generation(&state, result),
        move || {
            log_error("Private key generation worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't generate the key.")));
        },
    );
}

#[cfg(feature = "fido-ui")]
fn fido_key_generation_error(error: PrivateKeyError) -> FidoKeyGenerationError {
    let kind = error.fido_error_kind();
    let user_message = error.import_message();
    FidoKeyGenerationError::new(kind, error.to_string(), user_message)
}

fn clear_private_key_generation_form(state: &KeyManagementUiState) {
    state.widgets.private_key_name_row.set_text("");
    state.widgets.private_key_email_row.set_text("");
    state.widgets.private_key_password_row.set_text("");
    state.widgets.private_key_confirm_row.set_text("");
}

fn set_private_key_generation_loading(state: &KeyManagementUiState, loading: bool) {
    state.private_generation_in_flight.set(loading);
    let visible_child: &adw::gtk::Widget = if loading {
        state.widgets.private_key_loading.upcast_ref()
    } else {
        state.widgets.private_key_form.upcast_ref()
    };
    state
        .widgets
        .private_key_stack
        .set_visible_child(visible_child);
}

fn show_private_key_generation_page(state: &KeyManagementUiState) {
    if !state.standard_actions_allowed() {
        return;
    }

    state.mark_generation_page_opened();
    push_navigation_page_if_needed(&state.navigation, &state.widgets.private_key_page);

    if state.private_generation_in_flight.get() {
        set_private_key_generation_loading(state, true);
        return;
    }

    clear_private_key_generation_form(state);
    set_private_key_generation_loading(state, false);
    state.widgets.private_key_name_row.grab_focus();
}

fn connect_private_key_generation_submit(state: &KeyManagementUiState) {
    let name_row = state.widgets.private_key_name_row.clone();
    let email_row = state.widgets.private_key_email_row.clone();
    let password_row = state.widgets.private_key_password_row.clone();
    let confirm_row = state.widgets.private_key_confirm_row.clone();
    connect_private_apply_visibility(&name_row, &email_row, &password_row, &confirm_row);

    let state = state.clone();
    confirm_row.connect_apply(move |confirmation| {
        let request = match validate_private_key_generation_request(
            &name_row.text(),
            &email_row.text(),
            &password_row.text(),
            &confirmation.text(),
        ) {
            Ok(request) => request,
            Err(message) => {
                state.overlay.add_toast(Toast::new(&gettext(message)));
                return;
            }
        };

        start_private_key_generation(&state, request);
    });
}

fn finish_private_key_import(
    state: &KeyManagementUiState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => {
            state.notify_key_changed();
            state
                .overlay
                .add_toast(Toast::new(&gettext("Key imported.")));
        }
        Err(err) => {
            log_error(format!("Failed to import private key: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

fn start_private_key_import(
    state: &KeyManagementUiState,
    bytes: Vec<u8>,
    passphrase: Option<SecretString>,
) {
    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_progress_dialog(
        &state.window,
        "Importing key",
        None,
        "Wait a moment.",
    ));
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || {
            import_ripasso_private_key_bytes(
                &bytes,
                passphrase
                    .as_ref()
                    .map(|passphrase| passphrase.expose_secret()),
            )
        },
        move || progress_dialog.force_close(),
        move |result| finish_private_key_import(&state, result),
        move || {
            log_error("Private key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't import the key.")));
        },
    );
}

fn prompt_private_key_passphrase(state: &KeyManagementUiState, bytes: Vec<u8>) {
    let bytes = Rc::new(bytes);
    let window = state.window.clone();
    let overlay = state.overlay.clone();
    let state = state.clone();
    present_private_key_password_dialog(&window, &overlay, "Unlock key", None, move |passphrase| {
        start_private_key_import(&state, bytes.as_slice().to_vec(), Some(passphrase));
    });
}

fn import_private_key_bytes(state: &KeyManagementUiState, bytes: Vec<u8>) {
    match ripasso_private_key_requires_passphrase(&bytes) {
        Ok(true) => prompt_private_key_passphrase(state, bytes),
        Ok(false) => start_private_key_import(state, bytes, None),
        Err(err) => {
            log_error(format!("Failed to inspect private key: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext(err.inspection_message())));
        }
    }
}

fn open_private_key_picker(state: &KeyManagementUiState) {
    if !state.standard_actions_allowed() {
        return;
    }

    let state_for_response = state.clone();
    choose_file_bytes(
        &state.window,
        "Import private key",
        "Import",
        &state.overlay,
        "Failed to read the selected private key file",
        "Couldn't read that file.",
        move |bytes| import_private_key_bytes(&state_for_response, bytes),
    );
}

fn import_private_key_from_clipboard(state: &KeyManagementUiState) {
    if !state.standard_actions_allowed() {
        return;
    }

    let state_for_response = state.clone();
    (state.ports.read_clipboard_text)(Rc::new(move |result| match result {
        Ok(Some(text)) if !text.trim().is_empty() => {
            import_private_key_bytes(&state_for_response, text.as_bytes().to_vec());
        }
        Ok(_) => state_for_response
            .overlay
            .add_toast(Toast::new(&gettext("Clipboard does not contain a key."))),
        Err(err) => {
            log_error(format!("Failed to read private key from clipboard: {err}"));
            state_for_response
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't read the clipboard.")));
        }
    }));
}

pub(super) fn connect_controls(state: &KeyManagementUiState) {
    connect_generation_autofill_rows(
        &state.widgets.private_key_name_row,
        &state.widgets.private_key_email_row,
    );
    connect_private_key_generation_submit(state);

    let row = state.widgets.generate_private_key_row.clone();
    let state_for_generation = state.clone();
    connect_row_action(&row, move || {
        show_private_key_generation_page(&state_for_generation);
    });

    #[cfg(feature = "fido-ui")]
    {
        let state_for_allowed = state.clone();
        let state_for_generated = state.clone();
        state.fido.connect_generation_workflow(
            &state.window,
            &state.overlay,
            FidoKeyGenerationUiPorts {
                actions_allowed: Rc::new(move || state_for_allowed.standard_actions_allowed()),
                generate: Arc::new(|pin| {
                    generate_fido2_private_key(pin.as_ref().map(|pin| pin.expose_secret()))
                        .map(|_| ())
                        .map_err(fido_key_generation_error)
                }),
                set_pin_and_generate: Arc::new(|pin| {
                    set_fido2_security_key_pin(pin.expose_secret())
                        .map_err(fido_key_generation_error)?;
                    generate_fido2_private_key(Some(pin.expose_secret()))
                        .map(|_| ())
                        .map_err(fido_key_generation_error)
                }),
                on_generated: Rc::new(move || state_for_generated.notify_key_changed()),
            },
        );
    }

    let clipboard_row = state.widgets.import_clipboard_row.clone();
    let state_for_clipboard = state.clone();
    connect_row_action(&clipboard_row, move || {
        import_private_key_from_clipboard(&state_for_clipboard);
    });

    let file_row = state.widgets.import_file_row.clone();
    let state_for_file = state.clone();
    connect_row_action(&file_row, move || {
        open_private_key_picker(&state_for_file);
    });
}

#[cfg(test)]
mod tests {
    use super::validate_private_key_generation_request;

    #[test]
    fn generation_request_requires_name_email_and_matching_passwords() {
        assert_eq!(
            validate_private_key_generation_request("", "user@example.com", "hunter2", "hunter2"),
            Err("Enter a name.")
        );
        assert_eq!(
            validate_private_key_generation_request("User", "", "hunter2", "hunter2"),
            Err("Enter an email address.")
        );
        assert_eq!(
            validate_private_key_generation_request("User", "invalid", "hunter2", "hunter2"),
            Err("Enter a valid email address.")
        );
        assert_eq!(
            validate_private_key_generation_request("User", "user@example.com", "hunter2", "other"),
            Err("The passwords do not match.")
        );
    }
}
