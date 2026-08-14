//! Subject-neutral clipboard, copy-feedback, and copy/QR controls.

use adw::gtk::{gdk::Display, Button, Widget};
use adw::{glib, prelude::*, EntryRow, PasswordEntryRow, Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use std::time::Duration;

use crate::qr_code::{connect_copy_and_qr_buttons_with, copy_qr_button_group};
use crate::ui::flat_icon_button_with_tooltip;

const COPY_BUTTON_ICON_NAME: &str = "edit-copy-symbolic";
const COPIED_BUTTON_ICON_NAME: &str = "object-select-symbolic";
const COPY_BUTTON_FEEDBACK_MS: u64 = 1200;

fn show_clipboard_unavailable_toast(overlay: &ToastOverlay) {
    overlay.add_toast(Toast::new(&gettext("Clipboard unavailable.")));
}

pub fn set_copy_button_loading(button: Option<&Button>, loading: bool) {
    let Some(button) = button else {
        return;
    };
    button.set_sensitive(!loading);
}

pub fn show_copy_feedback(button: &Button) {
    button.set_icon_name(COPIED_BUTTON_ICON_NAME);

    let button = button.clone();
    glib::timeout_add_local_once(Duration::from_millis(COPY_BUTTON_FEEDBACK_MS), move || {
        button.set_icon_name(COPY_BUTTON_ICON_NAME);
    });
}

pub fn set_clipboard_text(text: &str, overlay: &ToastOverlay, button: Option<&Button>) -> bool {
    Display::default().map_or_else(
        || {
            show_clipboard_unavailable_toast(overlay);
            false
        },
        |display| {
            display.clipboard().set_text(text);
            if let Some(button) = button {
                show_copy_feedback(button);
            }
            true
        },
    )
}

pub fn connect_copy_button<F>(button: &Button, overlay: &ToastOverlay, text: F)
where
    F: Fn() -> String + 'static,
{
    let overlay = overlay.clone();
    let feedback_button = button.clone();
    button.connect_clicked(move |_| {
        let _ = set_clipboard_text(&text(), &overlay, Some(&feedback_button));
    });
}

pub fn connect_copy_and_qr_buttons<F>(
    copy_button: &Button,
    qr_button: &Button,
    overlay: &ToastOverlay,
    text: F,
) where
    F: Fn() -> String + 'static,
{
    connect_copy_and_qr_buttons_with(
        copy_button,
        qr_button,
        overlay,
        text,
        |button, overlay, text| connect_copy_button(button, overlay, move || text()),
    );
}

pub fn add_copy_suffix<W>(widget: &W, text: impl Fn() -> String + 'static, overlay: &ToastOverlay)
where
    W: IsA<Widget> + Clone,
{
    let copy_button = flat_icon_button_with_tooltip(COPY_BUTTON_ICON_NAME, "Copy value");
    let (button_group, qr_button) = copy_qr_button_group(&copy_button, "Show value as QR code");
    connect_copy_and_qr_buttons_with(
        &copy_button,
        &qr_button,
        overlay,
        text,
        |button, overlay, text| connect_copy_button(button, overlay, move || text()),
    );

    if let Some(row) = widget.dynamic_cast_ref::<EntryRow>() {
        row.add_suffix(&button_group);
    } else if let Some(row) = widget.dynamic_cast_ref::<PasswordEntryRow>() {
        row.add_suffix(&button_group);
    }
}
