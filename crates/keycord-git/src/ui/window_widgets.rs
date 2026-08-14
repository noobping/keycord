//! Git-owned widgets loaded from the composed application UI.

use adw::gtk::{
    Box as GtkBox, Builder, Button, Image, MenuButton, Popover, ScrolledWindow, SearchEntry,
    Spinner, Stack, Widget,
};
use adw::prelude::*;
use adw::{
    ActionRow, NavigationPage, NavigationView, PreferencesGroup, PreferencesPage, StatusPage,
};
use keycord_preferences::ui::{PreferencesPageSearchState, SearchablePreferencesGroup};
use keycord_shell::ui::{configure_touch_friendly_search_entry, required_builder_object};
use std::cell::RefCell;
use std::rc::Rc;

use super::focus::{
    connect_git_page_keyboard_navigation, focus_first_visible_git_page_target,
    visible_git_page_contains_focus,
};

#[derive(Clone)]
pub struct GitWindowWidgets {
    pub git_button: Button,
    pub store_git_page: NavigationPage,
    pub store_git_search_entry: SearchEntry,
    pub store_git_preferences_page: PreferencesPage,
    pub store_git_back_row: ActionRow,
    pub store_git_search_empty_group: PreferencesGroup,
    pub store_git_remotes_list: PreferencesGroup,
    pub store_git_actions_list: PreferencesGroup,
    pub store_git_status_list: PreferencesGroup,
    pub store_git_access_list: PreferencesGroup,
    pub git_busy_page: NavigationPage,
    pub git_busy_status: StatusPage,
    pub git_busy_show_logs_button: Button,
    pub tools_audit_filter_button: MenuButton,
    pub tools_audit_filter_popover: Popover,
    pub tools_audit_filter_store_box: GtkBox,
    pub tools_audit_filter_branch_box: GtkBox,
    pub tools_audit_row: ActionRow,
    pub tools_audit_suffix_stack: Stack,
    pub tools_audit_suffix_arrow: Image,
    pub tools_audit_spinner: Spinner,
    pub tools_audit_page: NavigationPage,
    pub tools_audit_search_entry: SearchEntry,
    pub tools_audit_stack: Stack,
    pub tools_audit_status: StatusPage,
    pub tools_audit_scrolled: ScrolledWindow,
    pub tools_audit_content: GtkBox,
}

impl GitWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        macro_rules! required {
            ($id:literal) => {
                required_builder_object(builder, $id)?
            };
        }

        Ok(Self {
            git_button: required!("git_button"),
            store_git_page: required!("store_git_page"),
            store_git_search_entry: required!("store_git_search_entry"),
            store_git_preferences_page: required!("store_git_preferences_page"),
            store_git_back_row: required!("store_git_back_row"),
            store_git_search_empty_group: required!("store_git_search_empty_group"),
            store_git_remotes_list: required!("store_git_remotes_list"),
            store_git_actions_list: required!("store_git_actions_list"),
            store_git_status_list: required!("store_git_status_list"),
            store_git_access_list: required!("store_git_access_list"),
            git_busy_page: required!("git_busy_page"),
            git_busy_status: required!("git_busy_status"),
            git_busy_show_logs_button: required!("git_busy_show_logs_button"),
            tools_audit_filter_button: required!("tools_audit_filter_button"),
            tools_audit_filter_popover: required!("tools_audit_filter_popover"),
            tools_audit_filter_store_box: required!("tools_audit_filter_store_box"),
            tools_audit_filter_branch_box: required!("tools_audit_filter_branch_box"),
            tools_audit_row: required!("tools_audit_row"),
            tools_audit_suffix_stack: required!("tools_audit_suffix_stack"),
            tools_audit_suffix_arrow: required!("tools_audit_suffix_arrow"),
            tools_audit_spinner: required!("tools_audit_spinner"),
            tools_audit_page: required!("tools_audit_page"),
            tools_audit_search_entry: required!("tools_audit_search_entry"),
            tools_audit_stack: required!("tools_audit_stack"),
            tools_audit_status: required!("tools_audit_status"),
            tools_audit_scrolled: required!("tools_audit_scrolled"),
            tools_audit_content: required!("tools_audit_content"),
        })
    }

    /// Build the Git-owned search projection for the store Git page.
    pub fn store_page_search_state(
        &self,
        remote_rows: Rc<RefCell<Vec<Widget>>>,
        action_rows: Rc<RefCell<Vec<Widget>>>,
        status_rows: Rc<RefCell<Vec<Widget>>>,
    ) -> PreferencesPageSearchState {
        PreferencesPageSearchState::new(
            &self.store_git_preferences_page,
            &self.store_git_search_entry,
            Some(&self.store_git_search_empty_group),
            vec![
                SearchablePreferencesGroup::with_tracked_widgets(
                    &self.store_git_remotes_list,
                    remote_rows,
                ),
                SearchablePreferencesGroup::with_tracked_widgets(
                    &self.store_git_actions_list,
                    action_rows,
                ),
                SearchablePreferencesGroup::with_tracked_widgets(
                    &self.store_git_status_list,
                    status_rows,
                ),
                SearchablePreferencesGroup::with_widgets(&self.store_git_access_list, Vec::new()),
            ],
        )
    }

    /// Apply Git-owned access to logs from the transient busy page.
    pub fn set_busy_logs_available(&self, available: bool) {
        self.git_busy_show_logs_button.set_visible(available);
    }

    pub fn connect_keyboard_navigation(&self) {
        connect_git_page_keyboard_navigation(&self.store_git_page, &self.tools_audit_page);
    }

    pub fn configure_search_entries(&self) {
        for entry in [&self.store_git_search_entry, &self.tools_audit_search_entry] {
            configure_touch_friendly_search_entry(entry);
        }
    }

    pub fn toggle_find_for_visible_page(
        &self,
        navigation: &NavigationView,
        find_button: &Button,
        on_audit_search_changed: &dyn Fn(),
    ) -> bool {
        if keycord_shell::ui::visible_navigation_page_is(navigation, &self.store_git_page) {
            keycord_shell::ui::toggle_page_search_entry(find_button, &self.store_git_search_entry);
            return true;
        }
        if keycord_shell::ui::visible_navigation_page_is(navigation, &self.tools_audit_page) {
            if !find_button.is_visible() {
                self.tools_audit_search_entry.set_visible(false);
            } else if self.tools_audit_search_entry.is_visible() {
                self.tools_audit_search_entry.set_visible(false);
                if !self.tools_audit_search_entry.text().is_empty() {
                    self.tools_audit_search_entry.set_text("");
                    on_audit_search_changed();
                }
            } else {
                self.tools_audit_search_entry.set_visible(true);
                self.tools_audit_search_entry.grab_focus();
            }
            return true;
        }
        keycord_shell::ui::visible_navigation_page_is(navigation, &self.git_busy_page)
    }

    pub fn focus_first_visible_page_target(&self, nav: &NavigationView) -> Option<bool> {
        focus_first_visible_git_page_target(
            nav,
            &self.store_git_page,
            &self.store_git_search_entry,
            &self.tools_audit_page,
        )
    }

    pub fn visible_page_contains_focus(&self, nav: &NavigationView) -> Option<bool> {
        visible_git_page_contains_focus(nav, &self.store_git_page, &self.tools_audit_page)
    }
}
