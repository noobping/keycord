//! Connects Keys' unlock UI to application refresh actions.

use adw::ToastOverlay;
use keycord_keys::ui::PrivateKeyUnlockUiPorts;
use std::rc::Rc;

pub fn prompt_private_key_unlock_for_action(
    overlay: &ToastOverlay,
    fingerprint: String,
    after_unlock: Rc<dyn Fn()>,
    on_finish: Rc<dyn Fn(bool)>,
) {
    let ports = PrivateKeyUnlockUiPorts::new(|window| {
        keycord_shell::actions::activate_widget_action(window, "win.reload-store-recipients-list");
        keycord_shell::actions::activate_widget_action(window, "win.reload-password-list");
    });
    keycord_keys::ui::prompt_private_key_unlock_for_action(
        overlay,
        fingerprint,
        ports,
        after_unlock,
        on_finish,
    );
}
