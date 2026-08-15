//! Keys-owned host synchronization policy and failure presentation.

use super::KeyManagementUiState;
use adw::Toast;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;

pub(super) fn handle_sync_failure(state: &KeyManagementUiState, err: &str) {
    log_error(format!("Failed to sync private keys with the host: {err}"));
    if let Err(save_err) = (state.ports.disable_private_key_sync)() {
        log_error(format!(
            "Failed to turn off private-key sync after an error: {save_err}",
        ));
    }
    state.overlay.add_toast(Toast::new(&gettext(
        "Couldn't keep private keys synced. Sync was turned off.",
    )));
}
