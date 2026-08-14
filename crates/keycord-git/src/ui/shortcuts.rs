//! Git-owned application accelerators.

use adw::prelude::*;
use adw::Application;
use keycord_runtime::capabilities::supports_host_command_features;
use keycord_stores::ui::management::NUMBERED_STORE_SHORTCUT_COUNT;

const GIT_SHORTCUTS: &[(&str, &str)] = &[
    ("win.synchronize", "<primary><shift>s"),
    ("win.open-git", "<primary>g"),
];

pub fn configure_git_shortcuts(app: &Application) {
    for (action, accelerator) in GIT_SHORTCUTS {
        app.set_accels_for_action(action, &[*accelerator]);
    }

    if !supports_host_command_features() {
        return;
    }

    for slot in 1..=NUMBERED_STORE_SHORTCUT_COUNT {
        let action = format!("win.open-store-git-{slot}");
        let accelerator = format!("<primary><alt>{slot}");
        app.set_accels_for_action(&action, &[accelerator.as_str()]);
    }
}

#[cfg(test)]
mod tests {
    use super::GIT_SHORTCUTS;

    #[test]
    fn primary_git_shortcuts_are_owned_together() {
        assert_eq!(
            GIT_SHORTCUTS,
            &[
                ("win.synchronize", "<primary><shift>s"),
                ("win.open-git", "<primary>g"),
            ]
        );
    }
}
