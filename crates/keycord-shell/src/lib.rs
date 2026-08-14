//! Application shell and generic GTK integration for Keycord.

/// Application-level keyboard shortcut dialog assembled by the shell.
pub const SHORTCUTS_UI: &str = include_str!(concat!(env!("OUT_DIR"), "/shortcuts.ui"));

pub mod actions;
pub mod application;
pub mod background;
pub mod clipboard;
pub mod deferred;
pub mod file_picker;
pub mod filters;
#[cfg(feature = "logging")]
pub mod logs;
pub mod navigation;
pub mod object_data;
pub mod optional_permission;
pub mod qr_code;
#[cfg(target_os = "linux")]
pub mod theme;
pub mod ui;
pub mod uri;
mod window_widgets;

pub use window_widgets::{
    configure_shell_shortcuts, log_page_presentation, ShellWindowWidgets, WindowFocusCallback,
};

#[cfg(test)]
mod shortcut_ui_tests {
    use super::SHORTCUTS_UI;

    fn fnv1a(source: &str) -> u64 {
        source.bytes().fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        })
    }

    #[test]
    fn composed_shortcuts_match_the_pre_split_template() {
        assert!(!SHORTCUTS_UI.contains("keycord-shortcuts-fragment:"));
        assert_eq!(SHORTCUTS_UI.lines().count(), 293);
        assert_eq!(fnv1a(SHORTCUTS_UI), 15_643_906_158_518_002_199);
    }
}
