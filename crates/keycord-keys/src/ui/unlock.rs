use super::dialog::{present_private_key_unlock_dialog_with_close_handler, PrivateKeyDialogHandle};
#[cfg(feature = "fido-ui")]
use crate::set_fido2_security_key_pin;
use crate::{
    list_connected_smartcard_keys, list_ripasso_private_keys, ripasso_private_key_title,
    unlock_ripasso_private_key_for_session, ManagedRipassoPrivateKey, PrivateKeyError,
    PrivateKeyUnlockKind, PrivateKeyUnlockRequest,
};
use adw::{glib, ApplicationWindow, Toast, ToastOverlay};
use keycord_runtime::diagnostics::log_error;
use keycord_runtime::i18n::gettext;
use keycord_shell::background::spawn_result_task_with_finalizer;
use keycord_shell::ui::{application_window_for_widget, build_progress_dialog};
#[cfg(feature = "fido-ui")]
use secrecy::ExposeSecret;
use std::rc::Rc;

/// Application-owned effects performed after a key is unlocked.
#[derive(Clone)]
pub struct PrivateKeyUnlockUiPorts {
    reload_after_unlock: Rc<dyn Fn(&ApplicationWindow)>,
}

impl PrivateKeyUnlockUiPorts {
    pub fn new(reload_after_unlock: impl Fn(&ApplicationWindow) + 'static) -> Self {
        Self {
            reload_after_unlock: Rc::new(reload_after_unlock),
        }
    }
}

fn show_unlock_failure_toast(overlay: &ToastOverlay) {
    overlay.add_toast(Toast::new(&gettext("Couldn't unlock the key.")));
}

fn finish_unlock_success(
    window: &ApplicationWindow,
    ports: &PrivateKeyUnlockUiPorts,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) {
    after_unlock();
    (ports.reload_after_unlock)(window);
    on_finish(true);
}

fn present_private_key_unlock_progress_dialog(
    window: &ApplicationWindow,
    subtitle: Option<&str>,
    kind: PrivateKeyUnlockKind,
) -> PrivateKeyDialogHandle {
    #[cfg(feature = "fido-ui")]
    if matches!(kind, PrivateKeyUnlockKind::Fido2SecurityKey) {
        return PrivateKeyDialogHandle::new(&keycord_fido::ui::present_progress_dialog(
            window,
            "Unlock key",
            subtitle,
        ));
    }

    let description = private_key_unlock_progress_description(kind);
    PrivateKeyDialogHandle::new(&build_progress_dialog(
        window,
        "Unlock key",
        subtitle,
        description,
    ))
}

#[cfg(feature = "fido-ui")]
fn present_fido2_pin_setup_progress_dialog(
    window: &ApplicationWindow,
    subtitle: Option<&str>,
) -> PrivateKeyDialogHandle {
    PrivateKeyDialogHandle::new(&keycord_fido::ui::present_progress_dialog(
        window,
        "Set security key PIN",
        subtitle,
    ))
}

fn private_key_unlock_progress_description(kind: PrivateKeyUnlockKind) -> &'static str {
    match kind {
        PrivateKeyUnlockKind::Fido2SecurityKey => unreachable!("FIDO UI is dispatched first"),
        PrivateKeyUnlockKind::HardwareOpenPgpCard => "Unlock your key to continue.",
        PrivateKeyUnlockKind::Password => "Wait a moment.",
    }
}

#[cfg(feature = "fido-ui")]
fn managed_fido2_unlock_enabled(kind: PrivateKeyUnlockKind) -> bool {
    matches!(kind, PrivateKeyUnlockKind::Fido2SecurityKey)
}

#[cfg(not(feature = "fido-ui"))]
const fn managed_fido2_unlock_enabled(_kind: PrivateKeyUnlockKind) -> bool {
    false
}

#[cfg(feature = "fido-ui")]
struct UnlockContinuation<'a> {
    ports: &'a PrivateKeyUnlockUiPorts,
    after_unlock: &'a Rc<dyn Fn()>,
    on_finish: &'a Rc<dyn Fn(bool)>,
}

#[cfg(feature = "fido-ui")]
fn handle_managed_fido2_retry(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    fingerprint: &str,
    error: &PrivateKeyError,
    allow_retry: bool,
    continuation: UnlockContinuation<'_>,
) -> bool {
    let Some(error_kind) = error.fido_error_kind() else {
        return false;
    };

    let key_title = match ripasso_private_key_title(fingerprint) {
        Ok(title) => Some(title),
        Err(err) => {
            log_error(format!(
                "Failed to read private key title for '{fingerprint}': {err}"
            ));
            None
        }
    };
    let overlay_for_pin_entry = overlay.clone();
    let fingerprint_for_pin_entry = fingerprint.to_string();
    let after_unlock_for_pin_entry = continuation.after_unlock.clone();
    let on_finish_for_pin_entry = continuation.on_finish.clone();
    let window_for_pin_entry = window.clone();
    let ports_for_pin_entry = continuation.ports.clone();

    let overlay_for_pin_setup = overlay.clone();
    let fingerprint_for_pin_setup = fingerprint.to_string();
    let after_unlock_for_pin_setup = continuation.after_unlock.clone();
    let on_finish_for_pin_setup = continuation.on_finish.clone();
    let window_for_pin_setup = window.clone();
    let ports_for_pin_setup = continuation.ports.clone();

    let on_finish_for_close = continuation.on_finish.clone();
    keycord_fido::ui::present_private_key_pin_retry_dialog(
        window,
        key_title.as_deref(),
        error_kind,
        allow_retry,
        move |pin| {
            start_private_key_unlock_for_action(
                &window_for_pin_entry,
                &overlay_for_pin_entry,
                fingerprint_for_pin_entry.clone(),
                PrivateKeyUnlockRequest::Fido2(Some(pin)),
                &ports_for_pin_entry,
                &after_unlock_for_pin_entry,
                &on_finish_for_pin_entry,
            );
        },
        move |pin| {
            start_private_key_fido2_pin_setup_for_action(
                &window_for_pin_setup,
                &overlay_for_pin_setup,
                fingerprint_for_pin_setup.clone(),
                pin,
                &ports_for_pin_setup,
                &after_unlock_for_pin_setup,
                &on_finish_for_pin_setup,
            );
        },
        move || on_finish_for_close(false),
    )
}

fn private_key_unlock_kind(fingerprint: &str) -> PrivateKeyUnlockKind {
    match list_ripasso_private_keys() {
        Ok(keys) => {
            if let Some(kind) = keys
                .into_iter()
                .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
                .map(|key| key.protection.into())
            {
                return kind;
            }
        }
        Err(err) => {
            log_error(format!(
                "Failed to read private key protection for '{fingerprint}': {err}"
            ));
        }
    }

    match list_connected_smartcard_keys() {
        Ok(keys) => keys
            .into_iter()
            .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
            .map(|_| PrivateKeyUnlockKind::HardwareOpenPgpCard)
            .unwrap_or(PrivateKeyUnlockKind::Password),
        Err(err) => {
            log_error(format!(
                "Failed to inspect connected smartcards for '{fingerprint}': {err}"
            ));
            PrivateKeyUnlockKind::Password
        }
    }
}

#[cfg(feature = "fido-ui")]
fn start_private_key_fido2_pin_setup_for_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    fingerprint: String,
    pin: secrecy::SecretString,
    ports: &PrivateKeyUnlockUiPorts,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) {
    let key_title = match ripasso_private_key_title(&fingerprint) {
        Ok(title) => Some(title),
        Err(err) => {
            log_error(format!(
                "Failed to read private key title for '{fingerprint}': {err}"
            ));
            None
        }
    };
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_result = window.clone();
    let after_unlock_for_result = after_unlock.clone();
    let on_finish_for_result = on_finish.clone();
    let on_finish_for_disconnect = on_finish.clone();
    let ports_for_result = ports.clone();
    let fingerprint_for_worker = fingerprint.clone();
    let progress_dialog = present_fido2_pin_setup_progress_dialog(window, key_title.as_deref());
    glib::idle_add_local_once(move || {
        spawn_result_task_with_finalizer(
            move || {
                set_fido2_security_key_pin(pin.expose_secret())?;
                unlock_ripasso_private_key_for_session(
                    &fingerprint_for_worker,
                    PrivateKeyUnlockRequest::Fido2(Some(pin)),
                )
            },
            move || progress_dialog.force_close(),
            move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
                Ok(_) => {
                    finish_unlock_success(
                        &window_for_result,
                        &ports_for_result,
                        &after_unlock_for_result,
                        &on_finish_for_result,
                    );
                }
                Err(err) => {
                    log_error(format!("Failed to set FIDO2 security key PIN: {err}"));
                    overlay.add_toast(Toast::new(&gettext(err.unlock_message())));
                    on_finish_for_result(false);
                }
            },
            move || {
                log_error("FIDO2 PIN setup worker disconnected unexpectedly.".to_string());
                show_unlock_failure_toast(&overlay_for_disconnect);
                on_finish_for_disconnect(false);
            },
        );
    });
}

fn start_private_key_unlock_for_action(
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    fingerprint: String,
    request: PrivateKeyUnlockRequest,
    ports: &PrivateKeyUnlockUiPorts,
    after_unlock: &Rc<dyn Fn()>,
    on_finish: &Rc<dyn Fn(bool)>,
) {
    let key_title = match ripasso_private_key_title(&fingerprint) {
        Ok(title) => Some(title),
        Err(err) => {
            log_error(format!(
                "Failed to read private key title for '{fingerprint}': {err}"
            ));
            None
        }
    };
    let kind = private_key_unlock_kind(&fingerprint);
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let window_for_result = window.clone();
    let after_unlock = after_unlock.clone();
    let on_finish_for_result = on_finish.clone();
    let on_finish_for_disconnect = on_finish.clone();
    let ports_for_result = ports.clone();
    #[cfg(feature = "fido-ui")]
    let allow_fido2_retry = matches!(request, PrivateKeyUnlockRequest::Fido2(None));
    let fingerprint_for_worker = fingerprint.clone();
    let progress_dialog =
        present_private_key_unlock_progress_dialog(window, key_title.as_deref(), kind);

    glib::idle_add_local_once(move || {
        spawn_result_task_with_finalizer(
            move || unlock_ripasso_private_key_for_session(&fingerprint_for_worker, request),
            move || progress_dialog.force_close(),
            move |result: Result<ManagedRipassoPrivateKey, PrivateKeyError>| match result {
                Ok(_) => {
                    finish_unlock_success(
                        &window_for_result,
                        &ports_for_result,
                        &after_unlock,
                        &on_finish_for_result,
                    );
                }
                #[cfg(feature = "fido-ui")]
                Err(err)
                    if handle_managed_fido2_retry(
                        &window_for_result,
                        &overlay,
                        &fingerprint,
                        &err,
                        allow_fido2_retry,
                        UnlockContinuation {
                            ports: &ports_for_result,
                            after_unlock: &after_unlock,
                            on_finish: &on_finish_for_result,
                        },
                    ) => {}
                Err(err) => {
                    log_error(format!("Failed to unlock ripasso private key: {err}"));
                    overlay.add_toast(Toast::new(&gettext(err.unlock_message())));
                    on_finish_for_result(false);
                }
            },
            move || {
                log_error("Private key unlock worker disconnected unexpectedly.".to_string());
                show_unlock_failure_toast(&overlay_for_disconnect);
                on_finish_for_disconnect(false);
            },
        );
    });
}

pub fn prompt_private_key_unlock_for_action(
    overlay: &ToastOverlay,
    fingerprint: String,
    ports: PrivateKeyUnlockUiPorts,
    after_unlock: Rc<dyn Fn()>,
    on_finish: Rc<dyn Fn(bool)>,
) {
    let Some(window) = application_window_for_widget(overlay) else {
        log_error(
            "Couldn't find the application window for the private key unlock dialog.".to_string(),
        );
        show_unlock_failure_toast(overlay);
        on_finish(false);
        return;
    };
    let key_title = match ripasso_private_key_title(&fingerprint) {
        Ok(title) => Some(title),
        Err(err) => {
            log_error(format!(
                "Failed to read private key title for '{fingerprint}': {err}"
            ));
            None
        }
    };
    let kind = private_key_unlock_kind(&fingerprint);
    if managed_fido2_unlock_enabled(kind) {
        start_private_key_unlock_for_action(
            &window,
            overlay,
            fingerprint,
            PrivateKeyUnlockRequest::Fido2(None),
            &ports,
            &after_unlock,
            &on_finish,
        );
        return;
    }

    let window_for_submit = window.clone();
    let overlay_for_submit = overlay.clone();
    let on_finish_for_close = on_finish.clone();
    let ports_for_submit = ports.clone();
    present_private_key_unlock_dialog_with_close_handler(
        &window,
        overlay,
        "Unlock key",
        key_title.as_deref(),
        kind,
        move |request| {
            start_private_key_unlock_for_action(
                &window_for_submit,
                &overlay_for_submit,
                fingerprint.clone(),
                request,
                &ports_for_submit,
                &after_unlock,
                &on_finish,
            );
        },
        move || on_finish_for_close(false),
    );
}

#[cfg(test)]
mod tests {
    use super::private_key_unlock_progress_description;
    use crate::PrivateKeyUnlockKind;

    #[test]
    fn generic_unlock_progress_copy_matches_non_fido_key_kinds() {
        assert_eq!(
            private_key_unlock_progress_description(PrivateKeyUnlockKind::HardwareOpenPgpCard),
            "Unlock your key to continue."
        );
        assert_eq!(
            private_key_unlock_progress_description(PrivateKeyUnlockKind::Password),
            "Wait a moment."
        );
    }
}
