//! Connects Passkey dialogs to the Entries import workflow.

use adw::ApplicationWindow;
use keycord_passkey::ui::{OpenPasskeyRequest, PasskeyDialogCallbacks};

pub fn present_open_passkey_request(window: &ApplicationWindow, opened: OpenPasskeyRequest) {
    let window_for_import = window.clone();
    let callbacks =
        PasskeyDialogCallbacks::new(keycord_runtime::i18n::gettext, move |credential| {
            crate::window::begin_passkey_import(&window_for_import, &credential)
        });
    keycord_passkey::ui::present_open_passkey_request(window, opened, callbacks);
}
