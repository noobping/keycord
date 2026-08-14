//! Keyboard focus policy for Git-owned pages.

use adw::gtk::{DirectionType, SearchEntry, Widget};
use adw::prelude::*;
use adw::{NavigationPage, NavigationView};
use keycord_shell::navigation::visible_navigation_page_is;
use keycord_shell::ui::{connect_vertical_arrow_navigation_for_buttons, widget_contains_focus};

pub fn connect_git_page_keyboard_navigation(
    store_page: &NavigationPage,
    audit_page: &NavigationPage,
) {
    connect_vertical_arrow_navigation_for_buttons(store_page);
    connect_vertical_arrow_navigation_for_buttons(audit_page);
}

pub fn focus_first_visible_git_page_target(
    nav: &NavigationView,
    store_page: &NavigationPage,
    store_search_entry: &SearchEntry,
    audit_page: &NavigationPage,
) -> Option<bool> {
    if visible_navigation_page_is(nav, audit_page) {
        return Some(audit_page.child_focus(DirectionType::Down));
    }
    if visible_navigation_page_is(nav, store_page) {
        return Some(if store_search_entry.is_visible() {
            store_search_entry.grab_focus()
        } else {
            store_page.child_focus(DirectionType::Down)
        });
    }
    None
}

pub fn visible_git_page_contains_focus(
    nav: &NavigationView,
    store_page: &NavigationPage,
    audit_page: &NavigationPage,
) -> Option<bool> {
    let page = if visible_navigation_page_is(nav, audit_page) {
        audit_page
    } else if visible_navigation_page_is(nav, store_page) {
        store_page
    } else {
        return None;
    };

    Some(widget_contains_focus(&page.clone().upcast::<Widget>()))
}
