//! Clipboard and QR presentation adapters for entry values.

use adw::gtk::Button;
use adw::prelude::*;
use adw::{Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_runtime::worker::spawn_worker;
use keycord_shell::background::spawn_result_task;
use keycord_shell::clipboard::{set_clipboard_text, set_copy_button_loading, show_copy_feedback};
use keycord_shell::qr_code::show_qr_code;
use std::rc::Rc;
use std::sync::Arc;

use crate::model::PassEntry;
use crate::PasswordEntryError;

pub type ReadEntryPasswordLine =
    Arc<dyn Fn(String, String) -> Result<String, PasswordEntryError> + Send + Sync>;
pub type ResolveEntryFingerprint =
    Arc<dyn Fn(String, String) -> Result<String, String> + Send + Sync>;
pub type CopyEntryWithHost = Arc<dyn Fn(PassEntry) -> Result<(), String> + Send + Sync>;
pub type PromptEntryUnlock = Rc<dyn Fn(&ToastOverlay, String, Rc<dyn Fn()>, Rc<dyn Fn(bool)>)>;

/// Application-owned services needed by the Entries clipboard controller.
#[derive(Clone)]
pub struct EntryClipboardPorts {
    uses_integrated_backend: Rc<dyn Fn() -> bool>,
    read_password_line: ReadEntryPasswordLine,
    preferred_fingerprint: ResolveEntryFingerprint,
    copy_with_host: CopyEntryWithHost,
    prompt_unlock: PromptEntryUnlock,
}

impl EntryClipboardPorts {
    pub fn new(
        uses_integrated_backend: impl Fn() -> bool + 'static,
        read_password_line: ReadEntryPasswordLine,
        preferred_fingerprint: ResolveEntryFingerprint,
        copy_with_host: CopyEntryWithHost,
        prompt_unlock: PromptEntryUnlock,
    ) -> Self {
        Self {
            uses_integrated_backend: Rc::new(uses_integrated_backend),
            read_password_line,
            preferred_fingerprint,
            copy_with_host,
            prompt_unlock,
        }
    }
}

fn copy_password_entry_with_host(
    item: PassEntry,
    button: Option<&Button>,
    ports: &EntryClipboardPorts,
) {
    if let Some(button) = button {
        show_copy_feedback(button);
    }

    let copy_with_host = ports.copy_with_host.clone();
    if let Err(err) = spawn_worker("clipboard-pass-copy", move || {
        if let Err(err) = copy_with_host(item) {
            log_error(format!(
                "Failed to copy password with the host command: {err}"
            ));
        }
    }) {
        log_error(format!("Failed to spawn clipboard copy worker: {err}"));
    }
}

fn handle_copy_password_error(
    item: &PassEntry,
    overlay: &ToastOverlay,
    button: Option<&Button>,
    error: &PasswordEntryError,
    ports: &EntryClipboardPorts,
) -> bool {
    if !matches!(error, PasswordEntryError::LockedPrivateKey(_)) {
        return false;
    }

    match (ports.preferred_fingerprint)(item.store_path.clone(), item.label()) {
        Ok(fingerprint) => {
            let retry_overlay = overlay.clone();
            let retry_item = item.clone();
            let retry_button = button.cloned();
            let finish_button = button.cloned();
            let retry_ports = ports.clone();
            (ports.prompt_unlock)(
                overlay,
                fingerprint,
                Rc::new(move || {
                    copy_password_entry_to_clipboard_via_read(
                        retry_item.clone(),
                        retry_overlay.clone(),
                        retry_button.clone(),
                        &retry_ports,
                    );
                }),
                Rc::new(move |success| {
                    if !success {
                        set_copy_button_loading(finish_button.as_ref(), false);
                    }
                }),
            );
            true
        }
        Err(resolve_err) => {
            log_error(format!(
                "Failed to resolve the private key for copy retry: {resolve_err}"
            ));
            false
        }
    }
}

pub fn copy_password_entry_to_clipboard_via_read(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
    ports: &EntryClipboardPorts,
) {
    set_copy_button_loading(button.as_ref(), true);
    let overlay_for_disconnect = overlay.clone();
    let button_for_disconnect = button.clone();
    let task_item = item.clone();
    let read_password_line = ports.read_password_line.clone();
    let result_ports = ports.clone();
    spawn_result_task(
        move || read_password_line(task_item.store_path.clone(), task_item.label()),
        move |result| match result {
            Ok(password) => {
                if set_clipboard_text(&password, &overlay, button.as_ref()) {
                    overlay.add_toast(Toast::new(&gettext("Copied.")));
                }
                set_copy_button_loading(button.as_ref(), false);
            }
            Err(err) => {
                log_error(format!("Failed to copy password entry: {err}"));
                if handle_copy_password_error(&item, &overlay, button.as_ref(), &err, &result_ports)
                {
                    return;
                }
                set_copy_button_loading(button.as_ref(), false);
                overlay.add_toast(Toast::new(&gettext("Couldn't copy the password.")));
            }
        },
        move || {
            set_copy_button_loading(button_for_disconnect.as_ref(), false);
            overlay_for_disconnect.add_toast(Toast::new(&gettext("Couldn't copy the password.")));
        },
    );
}

pub fn copy_password_entry_to_clipboard(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Option<Button>,
    ports: &EntryClipboardPorts,
) {
    if (ports.uses_integrated_backend)() {
        copy_password_entry_to_clipboard_via_read(item, overlay, button, ports);
    } else {
        copy_password_entry_with_host(item, button.as_ref(), ports);
    }
}

fn handle_password_qr_error(
    item: &PassEntry,
    overlay: &ToastOverlay,
    button: &Button,
    error: &PasswordEntryError,
    ports: &EntryClipboardPorts,
) -> bool {
    if !matches!(error, PasswordEntryError::LockedPrivateKey(_)) {
        return false;
    }

    match (ports.preferred_fingerprint)(item.store_path.clone(), item.label()) {
        Ok(fingerprint) => {
            let retry_overlay = overlay.clone();
            let retry_item = item.clone();
            let retry_button = button.clone();
            let finish_button = button.clone();
            let retry_ports = ports.clone();
            (ports.prompt_unlock)(
                overlay,
                fingerprint,
                Rc::new(move || {
                    show_password_entry_qr(
                        retry_item.clone(),
                        retry_overlay.clone(),
                        retry_button.clone(),
                        &retry_ports,
                    );
                }),
                Rc::new(move |success| {
                    if !success {
                        finish_button.set_sensitive(true);
                    }
                }),
            );
            true
        }
        Err(resolve_err) => {
            log_error(format!(
                "Failed to resolve the private key for QR retry: {resolve_err}"
            ));
            false
        }
    }
}

pub fn show_password_entry_qr(
    item: PassEntry,
    overlay: ToastOverlay,
    button: Button,
    ports: &EntryClipboardPorts,
) {
    button.set_sensitive(false);
    let overlay_for_disconnect = overlay.clone();
    let button_for_disconnect = button.clone();
    let task_item = item.clone();
    let read_password_line = ports.read_password_line.clone();
    let result_ports = ports.clone();
    spawn_result_task(
        move || read_password_line(task_item.store_path.clone(), task_item.label()),
        move |result| match result {
            Ok(password) => {
                show_qr_code(&password, &overlay, &button);
                button.set_sensitive(true);
            }
            Err(err) => {
                log_error(format!("Failed to read password entry for QR code: {err}"));
                if handle_password_qr_error(&item, &overlay, &button, &err, &result_ports) {
                    return;
                }
                button.set_sensitive(true);
                overlay.add_toast(Toast::new(&gettext(
                    "Couldn't show the password as a QR code.",
                )));
            }
        },
        move || {
            button_for_disconnect.set_sensitive(true);
            overlay_for_disconnect.add_toast(Toast::new(&gettext(
                "Couldn't show the password as a QR code.",
            )));
        },
    );
}
