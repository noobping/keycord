//! Feature-disabled Git action implementation.

use super::GitActionState;
use keycord_shell::actions::register_window_action;
use keycord_stores::ui::management::NUMBERED_STORE_SHORTCUT_COUNT;

pub fn clone_store_repository(_url: &str, _store_root: &str) -> Result<(), String> {
    Err("Host command features are only available on Linux.".to_string())
}

pub fn register_open_git_action(state: &GitActionState) {
    let window = state.window.clone();
    register_window_action(&window, "git-clone", || {});
    register_window_action(&window, "open-git", || {});

    for slot in 1..=NUMBERED_STORE_SHORTCUT_COUNT {
        register_window_action(&window, &format!("open-store-git-{slot}"), || {});
    }
}

pub fn register_synchronize_action(state: &GitActionState) {
    let window = state.window.clone();
    register_window_action(&window, "synchronize", || {});
}

pub fn handle_git_busy_back(_state: &GitActionState) -> bool {
    false
}
