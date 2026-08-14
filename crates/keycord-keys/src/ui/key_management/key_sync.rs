//! Keys-owned host synchronization policy and failure presentation.

use super::KeyManagementUiState;
use adw::Toast;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum KeySyncOutcome {
    Disabled,
    Succeeded,
    Failed,
}

fn handle_sync_failure(state: &KeyManagementUiState, err: &str) {
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

fn sync_if_enabled(
    state: &KeyManagementUiState,
    sync: &dyn Fn() -> Result<(), String>,
) -> KeySyncOutcome {
    if !(state.ports.private_key_sync_enabled)() {
        return KeySyncOutcome::Disabled;
    }
    match sync() {
        Ok(()) => KeySyncOutcome::Succeeded,
        Err(err) => {
            handle_sync_failure(state, &err);
            KeySyncOutcome::Failed
        }
    }
}

pub(super) fn sync_from_host(state: &KeyManagementUiState) -> KeySyncOutcome {
    sync_if_enabled(state, state.ports.sync_private_keys_from_host.as_ref())
}

pub(super) fn sync_to_host(state: &KeyManagementUiState) -> KeySyncOutcome {
    sync_if_enabled(state, state.ports.sync_private_keys_to_host.as_ref())
}
