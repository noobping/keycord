//! Composition of subject-owned keyboard, focus, and window-shell policy.

use super::widgets::WindowWidgets;
use crate::window::navigation::WindowNavigationState;
use adw::glib::Propagation;
use adw::gtk::Widget;
use adw::prelude::*;
use adw::ApplicationWindow;
use keycord_entries::ui::widgets::{focus_visible_entry_page, visible_entry_page_contains_focus};
use keycord_preferences::Preferences;
use keycord_shell::WindowFocusCallback;
use std::rc::Rc;

pub(super) fn initialize_window_chrome(widgets: &WindowWidgets, preferences: &Preferences) {
    configure_search_entries(widgets);
    restore_window_size(&widgets.shell.window, preferences);
    connect_window_size_persistence(&widgets.shell.window);
}

pub(super) fn connect_window_keyboard_navigation(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) {
    let primary_menu: Widget = widgets.shell.primary_menu.clone().upcast();
    widgets.entries.connect_keyboard_navigation();
    widgets
        .preferences
        .connect_keyboard_navigation(&primary_menu);
    widgets.tool_hub.connect_keyboard_navigation(&primary_menu);
    widgets.stores.connect_keyboard_navigation();
    widgets.git.connect_keyboard_navigation();
    widgets.keys.connect_keyboard_navigation();

    let (contains_focus, focus_target) = focus_callbacks(widgets, navigation);
    widgets
        .shell
        .connect_page_focus_navigation(contains_focus, focus_target);
}

pub(super) fn schedule_initial_focus(widgets: &WindowWidgets, navigation: &WindowNavigationState) {
    let (contains_focus, focus_target) = focus_callbacks(widgets, navigation);
    widgets
        .shell
        .schedule_page_focus(contains_focus, focus_target);
}

fn focus_callbacks(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) -> (WindowFocusCallback, WindowFocusCallback) {
    let widgets_for_contains = widgets.clone();
    let navigation_for_contains = navigation.clone();
    let contains_focus = Rc::new(move || {
        visible_page_contains_focus(&widgets_for_contains, &navigation_for_contains)
    });

    let widgets_for_target = widgets.clone();
    let navigation_for_target = navigation.clone();
    let focus_target = Rc::new(move || {
        focus_first_visible_page_target(&widgets_for_target, &navigation_for_target)
    });
    (contains_focus, focus_target)
}

fn focus_first_visible_page_target(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) -> bool {
    let nav = &navigation.nav;
    if let Some(focused) = focus_visible_entry_page(&widgets.entries, nav) {
        return focused;
    }
    if let Some(focused) = widgets.preferences.focus_first_visible_page_target(nav) {
        return focused;
    }
    if let Some(focused) = widgets.tool_hub.focus_first_visible_page_target(nav) {
        return focused;
    }
    if let Some(focused) = widgets.docs.focus_first_visible_page_target(nav) {
        return focused;
    }
    if let Some(focused) = widgets.git.focus_first_visible_page_target(nav) {
        return focused;
    }
    if let Some(focused) = widgets.stores.focus_first_visible_page_target(nav) {
        return focused;
    }
    if let Some(focused) = widgets.keys.focus_first_visible_page_target(nav) {
        return focused;
    }
    widgets
        .shell
        .focus_first_visible_page_target(nav)
        .unwrap_or(false)
}

fn visible_page_contains_focus(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) -> bool {
    let nav = &navigation.nav;
    if let Some(contains_focus) = visible_entry_page_contains_focus(&widgets.entries, nav) {
        return contains_focus;
    }
    if let Some(contains_focus) = widgets.preferences.visible_page_contains_focus(nav) {
        return contains_focus;
    }
    if let Some(contains_focus) = widgets.tool_hub.visible_page_contains_focus(nav) {
        return contains_focus;
    }
    if let Some(contains_focus) = widgets.docs.visible_page_contains_focus(nav) {
        return contains_focus;
    }
    if let Some(contains_focus) = widgets.git.visible_page_contains_focus(nav) {
        return contains_focus;
    }
    if let Some(contains_focus) = widgets.stores.visible_page_contains_focus(nav) {
        return contains_focus;
    }
    if let Some(contains_focus) = widgets.keys.visible_page_contains_focus(nav) {
        return contains_focus;
    }
    widgets
        .shell
        .visible_page_contains_focus(nav)
        .unwrap_or(false)
}

fn restore_window_size(window: &ApplicationWindow, preferences: &Preferences) {
    let (width, height) = preferences.window_size();
    window.set_default_size(width, height);
}

fn configure_search_entries(widgets: &WindowWidgets) {
    widgets.entries.configure_search_entries();
    widgets.preferences.configure_search_entries();
    widgets.tool_hub.configure_search_entries();
    widgets.docs.configure_search_entries();
    widgets.git.configure_search_entries();
    widgets.stores.configure_search_entries();
}

fn connect_window_size_persistence(window: &ApplicationWindow) {
    let preferences = Preferences::new();
    window.connect_close_request(move |window| {
        let width = window.width();
        let height = window.height();
        if width > 0 && height > 0 {
            let _ = preferences.set_window_size(width, height);
        }
        Propagation::Proceed
    });
}
