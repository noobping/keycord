//! Root-owned widgets for the cross-subject tool hub container.

use adw::gtk::{Builder, Button, ListBox, SearchEntry, Widget};
use adw::prelude::*;
use adw::{NavigationPage, NavigationView, PreferencesGroup};
use keycord_shell::ui::{
    configure_touch_friendly_search_entry,
    connect_ordered_keyboard_focusable_search_list_arrow_navigation,
    connect_ordered_list_arrow_navigation, connect_vertical_arrow_navigation_for_buttons,
    focus_first_keyboard_focusable_list_row, list_row_is_keyboard_focusable,
    required_builder_object, visible_navigation_page_is, widget_contains_focus,
};

#[derive(Clone)]
pub struct ToolHubWindowWidgets {
    pub page: NavigationPage,
    pub search_entry: SearchEntry,
    pub primary_group: PreferencesGroup,
    pub primary_list: ListBox,
    pub information_group: PreferencesGroup,
    pub search_empty_group: PreferencesGroup,
    pub information_list: ListBox,
}

impl ToolHubWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        Ok(Self {
            page: required_builder_object(builder, "tools_page")?,
            search_entry: required_builder_object(builder, "tools_search_entry")?,
            primary_group: required_builder_object(builder, "tools_primary_group")?,
            primary_list: required_builder_object(builder, "tools_list")?,
            information_group: required_builder_object(builder, "tools_information_group")?,
            search_empty_group: required_builder_object(builder, "tools_search_empty_group")?,
            information_list: required_builder_object(builder, "tools_logs_list")?,
        })
    }

    pub fn connect_keyboard_navigation(&self, primary_menu_button: &Widget) {
        connect_vertical_arrow_navigation_for_buttons(&self.page);
        let lists = [self.primary_list.clone(), self.information_list.clone()];
        connect_ordered_list_arrow_navigation(
            &lists,
            Some(primary_menu_button),
            list_row_is_keyboard_focusable,
        );
        connect_ordered_keyboard_focusable_search_list_arrow_navigation(&lists, &self.search_entry);
    }

    pub fn configure_search_entries(&self) {
        configure_touch_friendly_search_entry(&self.search_entry);
    }

    pub fn toggle_find_for_visible_page(
        &self,
        navigation: &NavigationView,
        find_button: &Button,
    ) -> bool {
        if !visible_navigation_page_is(navigation, &self.page) {
            return false;
        }
        keycord_shell::ui::toggle_page_search_entry(find_button, &self.search_entry);
        true
    }

    pub fn focus_first_visible_page_target(&self, navigation: &NavigationView) -> Option<bool> {
        if !visible_navigation_page_is(navigation, &self.page) {
            return None;
        }
        Some(
            (self.search_entry.is_visible() && self.search_entry.grab_focus())
                || focus_first_keyboard_focusable_list_row(&self.primary_list)
                || focus_first_keyboard_focusable_list_row(&self.information_list),
        )
    }

    pub fn visible_page_contains_focus(&self, navigation: &NavigationView) -> Option<bool> {
        visible_navigation_page_is(navigation, &self.page)
            .then(|| widget_contains_focus(&self.page.clone().upcast()))
    }
}
