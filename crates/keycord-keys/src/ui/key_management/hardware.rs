//! Smartcard discovery, import, and on-device OpenPGP generation UI.

use super::form::{
    connect_generation_autofill_rows, connect_hardware_apply_visibility, validate_name_and_email,
};
use super::KeyManagementUiState;
use crate::ui::PrivateKeyDialogHandle;
use crate::{
    discover_ripasso_hardware_keys, generate_ripasso_hardware_key,
    import_ripasso_hardware_key_bytes, DiscoveredHardwareToken, ManagedRipassoHardwareKey,
    ManagedRipassoPrivateKey, PrivateKeyError,
};
use adw::prelude::*;
use adw::Toast;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::background::spawn_result_task_with_finalizer;
use keycord_shell::file_picker::choose_file_bytes;
use keycord_shell::ui::{
    build_progress_dialog, connect_row_action, push_navigation_page_if_needed,
};
use secrecy::SecretString;

#[derive(Clone, Debug)]
struct HardwareKeyGenerationRequest {
    name: String,
    email: String,
    admin_pin: SecretString,
    user_pin: SecretString,
}

fn build_hardware_key_generation_request(
    name: &str,
    email: &str,
    admin_pin: &str,
    user_pin: &str,
) -> Result<HardwareKeyGenerationRequest, &'static str> {
    let (name, email) = validate_name_and_email(name, email)?;
    if admin_pin.trim().is_empty() {
        return Err("Enter the hardware key admin PIN.");
    }
    if user_pin.trim().is_empty() {
        return Err("Enter the new hardware key PIN.");
    }

    Ok(HardwareKeyGenerationRequest {
        name,
        email,
        admin_pin: SecretString::from(admin_pin),
        user_pin: SecretString::from(user_pin),
    })
}

fn hardware_key_from_token(token: &DiscoveredHardwareToken) -> ManagedRipassoHardwareKey {
    ManagedRipassoHardwareKey {
        ident: token.ident.clone(),
        signing_fingerprint: token.signing_fingerprint.clone(),
        decryption_fingerprint: token.decryption_fingerprint.clone(),
        reader_hint: token.reader_hint.clone(),
    }
}

fn selected_hardware_token(state: &KeyManagementUiState) -> Option<DiscoveredHardwareToken> {
    match discover_ripasso_hardware_keys() {
        Ok(mut tokens) => match tokens.len() {
            0 => {
                state
                    .overlay
                    .add_toast(Toast::new(&gettext("Connect a hardware key first.")));
                None
            }
            1 => tokens.pop(),
            _ => {
                state.overlay.add_toast(Toast::new(&gettext(
                    "Connect only one hardware key before adding it.",
                )));
                None
            }
        },
        Err(err) => {
            log_error(format!("Failed to discover hardware keys: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't inspect the hardware key.")));
            None
        }
    }
}

fn finish_hardware_key_import(
    state: &KeyManagementUiState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(_) => {
            state.notify_key_changed();
            state
                .overlay
                .add_toast(Toast::new(&gettext("Hardware key added.")));
        }
        Err(err) => {
            log_error(format!("Failed to import hardware key: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

fn start_hardware_key_import(
    state: &KeyManagementUiState,
    bytes: Vec<u8>,
    hardware: ManagedRipassoHardwareKey,
) {
    let state = state.clone();
    let progress_dialog = PrivateKeyDialogHandle::new(&build_progress_dialog(
        &state.window,
        "Adding hardware key",
        None,
        "Wait a moment.",
    ));
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || import_ripasso_hardware_key_bytes(&bytes, hardware.clone()),
        move || progress_dialog.force_close(),
        move |result| finish_hardware_key_import(&state, result),
        move || {
            log_error("Hardware key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't add the hardware key.")));
        },
    );
}

fn start_hardware_key_generation(
    state: &KeyManagementUiState,
    request: HardwareKeyGenerationRequest,
) {
    let Some(token) = state.hardware_generation_token.borrow().clone() else {
        state
            .overlay
            .add_toast(Toast::new(&gettext("Connect a hardware key first.")));
        return;
    };

    set_hardware_key_generation_loading(state, true);
    let state = state.clone();
    let state_for_finalize = state.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || {
            let HardwareKeyGenerationRequest {
                name,
                email,
                admin_pin,
                user_pin,
            } = request;
            generate_ripasso_hardware_key(
                &token.ident,
                token.reader_hint.as_deref(),
                &name,
                &email,
                admin_pin,
                user_pin,
                true,
            )
        },
        move || set_hardware_key_generation_loading(&state_for_finalize, false),
        move |result| finish_hardware_key_generation(&state, result),
        move || {
            log_error("Hardware key generation worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't add the hardware key.")));
        },
    );
}

fn open_hardware_public_key_picker(
    state: &KeyManagementUiState,
    hardware: ManagedRipassoHardwareKey,
    title: &str,
) {
    let state_for_response = state.clone();
    choose_file_bytes(
        &state.window,
        title,
        "Import",
        &state.overlay,
        "Failed to read the selected hardware public key file",
        "Couldn't read that file.",
        move |bytes| {
            start_hardware_key_import(&state_for_response, bytes, hardware.clone());
        },
    );
}

fn clear_hardware_key_generation_form(state: &KeyManagementUiState) {
    state.widgets.hardware_key_name_row.set_text("");
    state.widgets.hardware_key_email_row.set_text("");
    state.widgets.hardware_key_admin_pin_row.set_text("");
    state.widgets.hardware_key_user_pin_row.set_text("");
}

fn set_hardware_key_generation_loading(state: &KeyManagementUiState, loading: bool) {
    state.hardware_generation_in_flight.set(loading);
    let visible_child: &adw::gtk::Widget = if loading {
        state.widgets.hardware_key_loading.upcast_ref()
    } else {
        state.widgets.hardware_key_form.upcast_ref()
    };
    state
        .widgets
        .hardware_key_stack
        .set_visible_child(visible_child);
}

fn finish_hardware_key_generation(
    state: &KeyManagementUiState,
    result: Result<ManagedRipassoPrivateKey, PrivateKeyError>,
) {
    match result {
        Ok(key) => {
            clear_hardware_key_generation_form(state);
            state.hardware_generation_token.borrow_mut().take();
            state.pop_generation_page_if_visible(&state.widgets.hardware_key_page);
            finish_hardware_key_import(state, Ok(key));
        }
        Err(err) => {
            log_error(format!("Failed to set up hardware key: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

fn show_hardware_key_generation_page(state: &KeyManagementUiState, token: DiscoveredHardwareToken) {
    state.mark_generation_page_opened();
    push_navigation_page_if_needed(&state.navigation, &state.widgets.hardware_key_page);
    state.hardware_generation_token.borrow_mut().replace(token);

    if state.hardware_generation_in_flight.get() {
        set_hardware_key_generation_loading(state, true);
        return;
    }

    clear_hardware_key_generation_form(state);
    set_hardware_key_generation_loading(state, false);
    state.widgets.hardware_key_name_row.grab_focus();
}

fn add_connected_hardware_key(state: &KeyManagementUiState) {
    if !state.standard_actions_allowed() {
        return;
    }

    let Some(token) = selected_hardware_token(state) else {
        return;
    };
    let hardware = hardware_key_from_token(&token);
    if let Some(bytes) = token.cardholder_certificate {
        start_hardware_key_import(state, bytes, hardware);
        return;
    }
    if token.signing_fingerprint.is_some() || token.decryption_fingerprint.is_some() {
        state.overlay.add_toast(Toast::new(&gettext(
            "This hardware key already has OpenPGP keys. Import the matching public key file instead.",
        )));
        return;
    }

    state.overlay.add_toast(Toast::new(&gettext(
        "This hardware key has no OpenPGP key yet. Use Set up new hardware key (Experimental) instead.",
    )));
}

fn setup_connected_hardware_key(state: &KeyManagementUiState) {
    if !state.standard_actions_allowed() {
        return;
    }

    let Some(token) = selected_hardware_token(state) else {
        return;
    };
    if token.cardholder_certificate.is_some()
        || token.signing_fingerprint.is_some()
        || token.decryption_fingerprint.is_some()
    {
        state.overlay.add_toast(Toast::new(&gettext(
            "This hardware key already has OpenPGP keys. Use Add hardware key (Experimental) or import the matching public key file instead.",
        )));
        return;
    }

    show_hardware_key_generation_page(state, token);
}

fn import_hardware_key_from_file(state: &KeyManagementUiState) {
    if !state.standard_actions_allowed() {
        return;
    }

    let Some(token) = selected_hardware_token(state) else {
        return;
    };
    open_hardware_public_key_picker(
        state,
        hardware_key_from_token(&token),
        "Import hardware public key (Experimental)",
    );
}

fn connect_hardware_key_generation_submit(state: &KeyManagementUiState) {
    let name_row = state.widgets.hardware_key_name_row.clone();
    let email_row = state.widgets.hardware_key_email_row.clone();
    let admin_pin_row = state.widgets.hardware_key_admin_pin_row.clone();
    let user_pin_row = state.widgets.hardware_key_user_pin_row.clone();
    connect_hardware_apply_visibility(&name_row, &email_row, &admin_pin_row, &user_pin_row);

    let state = state.clone();
    user_pin_row.connect_apply(move |user_pin| {
        let request = match build_hardware_key_generation_request(
            &name_row.text(),
            &email_row.text(),
            &admin_pin_row.text(),
            &user_pin.text(),
        ) {
            Ok(request) => request,
            Err(message) => {
                state.overlay.add_toast(Toast::new(&gettext(message)));
                return;
            }
        };

        start_hardware_key_generation(&state, request);
    });
}

pub(super) fn connect_controls(state: &KeyManagementUiState) {
    connect_generation_autofill_rows(
        &state.widgets.hardware_key_name_row,
        &state.widgets.hardware_key_email_row,
    );
    connect_hardware_key_generation_submit(state);

    let setup_row = state.widgets.setup_hardware_key_row.clone();
    let state_for_setup = state.clone();
    connect_row_action(&setup_row, move || {
        setup_connected_hardware_key(&state_for_setup);
    });

    let add_row = state.widgets.add_hardware_key_row.clone();
    let state_for_add = state.clone();
    connect_row_action(&add_row, move || {
        add_connected_hardware_key(&state_for_add);
    });

    let import_row = state.widgets.import_hardware_key_row.clone();
    let state_for_import = state.clone();
    connect_row_action(&import_row, move || {
        import_hardware_key_from_file(&state_for_import);
    });
}

#[cfg(test)]
mod tests {
    use super::build_hardware_key_generation_request;

    #[test]
    fn hardware_generation_request_validates_identity_and_pins() {
        assert_eq!(
            build_hardware_key_generation_request("", "user@example.com", "12345678", "123456")
                .unwrap_err(),
            "Enter a name."
        );
        assert_eq!(
            build_hardware_key_generation_request("User", "invalid", "12345678", "123456")
                .unwrap_err(),
            "Enter a valid email address."
        );
        assert_eq!(
            build_hardware_key_generation_request("User", "user@example.com", "", "123456")
                .unwrap_err(),
            "Enter the hardware key admin PIN."
        );
        assert_eq!(
            build_hardware_key_generation_request("User", "user@example.com", "12345678", "")
                .unwrap_err(),
            "Enter the new hardware key PIN."
        );
    }
}
