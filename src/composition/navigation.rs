//! Application policy for generic Shell navigation slots.

use keycord_preferences::Preferences;
use keycord_runtime::capabilities::has_host_permission;
use keycord_shell::navigation::{
    show_primary_page_chrome, PrimaryPageActionVisibility, WindowChrome, WindowChromeCallback,
    APP_WINDOW_TITLE,
};
use std::rc::Rc;

pub const APP_WINDOW_SUBTITLE: &str = "Browse and edit password stores";

pub fn show_root_page_chrome(chrome: &WindowChrome<'_>) {
    let has_stores = !Preferences::new().stores().is_empty();
    keycord_entries::ui::actions::configure_password_save_button(chrome.save);
    show_primary_page_chrome(
        chrome,
        PrimaryPageActionVisibility {
            add: has_stores,
            find: true,
            primary: !has_stores && has_host_permission(),
            secondary: !has_stores,
        },
        APP_WINDOW_TITLE,
        APP_WINDOW_SUBTITLE,
    );
}

pub fn root_page_chrome_callback() -> WindowChromeCallback {
    Rc::new(show_root_page_chrome)
}
