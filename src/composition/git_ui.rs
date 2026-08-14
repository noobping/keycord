//! Application-level policy adapters used by the Git UI.

use adw::ApplicationWindow;
use keycord_shell::actions::{activate_widget_action, set_window_action_enabled};
use keycord_stores::ui::management::NUMBERED_STORE_SHORTCUT_COUNT;

const APPLICATION_ACTIONS_BLOCKED_WHILE_BUSY: &[&str] = &[
    "context-save",
    "context-undo",
    "open-new-password",
    "toggle-find",
    "open-raw-pass-file",
    "save-password",
    "save-store-recipients",
    "open-preferences",
    "open-tools",
    "open-docs",
    "toggle-hidden-and-duplicates",
];

fn application_busy_action_names() -> Vec<String> {
    let mut actions = APPLICATION_ACTIONS_BLOCKED_WHILE_BUSY
        .iter()
        .map(|action| (*action).to_string())
        .collect::<Vec<_>>();
    for slot in 1..=NUMBERED_STORE_SHORTCUT_COUNT {
        actions.push(format!("open-store-recipients-{slot}"));
    }
    actions
}

pub fn set_application_busy(window: &ApplicationWindow, busy: bool) {
    for action in application_busy_action_names() {
        let _ = set_window_action_enabled(window, &action, !busy);
    }
}

pub fn refresh_related_views(window: &ApplicationWindow) {
    activate_widget_action(window, "win.reload-store-recipients-list");
    activate_widget_action(window, "win.reload-password-list");
}

#[cfg(test)]
mod tests {
    use super::application_busy_action_names;

    #[test]
    fn application_busy_policy_excludes_git_owned_actions() {
        let actions = application_busy_action_names();

        assert!(actions.iter().any(|action| action == "save-password"));
        assert!(actions
            .iter()
            .any(|action| action == "save-store-recipients"));
        assert!(actions
            .iter()
            .any(|action| action == "open-store-recipients-1"));
        assert!(actions.iter().all(|action| action != "git-clone"));
        assert!(actions.iter().all(|action| action != "open-git"));
        assert!(actions.iter().all(|action| action != "synchronize"));
        assert!(actions
            .iter()
            .all(|action| !action.starts_with("open-store-git-")));
    }
}
