//! Builder-owned widgets declared by Entries UI fragments.

use adw::gtk::{
    Box as GtkBox, Builder, Button, DirectionType, Image, Label, ListBox, MenuButton, Popover,
    Revealer, ScrolledWindow, SearchEntry, SpinButton, Spinner, Stack, TextView, ToggleButton,
};
use adw::prelude::*;
use adw::{ActionRow, EntryRow, NavigationPage, PasswordEntryRow, StatusPage};
use keycord_shell::ui::{
    configure_touch_friendly_search_entry, connect_horizontal_arrow_adjustment_for_spin_buttons,
    focus_first_keyboard_focusable_list_row, navigation_stack_is_root, required_builder_object,
    visible_navigation_page_is, widget_contains_focus,
};

use super::list::{focus_first_password_list_row, toggle_password_list_search};

#[derive(Clone)]
pub struct EntryWindowWidgets {
    pub add_button: Button,
    pub find_button: Button,
    pub save_button: Button,
    pub password_list_filter_button: MenuButton,
    pub password_list_filter_popover: Popover,
    pub password_list_filter_store_box: GtkBox,
    pub search_entry: SearchEntry,
    pub password_list_stack: Stack,
    pub password_list_status: StatusPage,
    pub password_list_spinner: Spinner,
    pub password_list_scrolled: ScrolledWindow,
    pub list: ListBox,
    pub password_page: NavigationPage,
    pub raw_text_page: NavigationPage,
    pub password_status: StatusPage,
    pub password_entry: PasswordEntryRow,
    pub password_analysis_label: Label,
    pub password_generator_settings_button: ToggleButton,
    pub password_generator_settings_revealer: Revealer,
    pub password_generator_length_spin: SpinButton,
    pub password_generator_min_lowercase_spin: SpinButton,
    pub password_generator_min_uppercase_spin: SpinButton,
    pub password_generator_min_numbers_spin: SpinButton,
    pub password_generator_min_symbols_spin: SpinButton,
    pub username_entry: EntryRow,
    pub otp_entry: PasswordEntryRow,
    pub add_field_row: EntryRow,
    pub apply_template_button: Button,
    pub clean_pass_file_button: Button,
    pub add_otp_button: Button,
    pub import_private_key_button: Button,
    pub editor_save_button: Button,
    pub copy_password_button: Button,
    pub qr_password_button: Button,
    pub copy_username_button: Button,
    pub qr_username_button: Button,
    pub copy_otp_button: Button,
    pub qr_otp_button: Button,
    pub text_view: TextView,
    pub dynamic_fields_box: GtkBox,
    pub open_raw_button: Button,
    pub tools_field_values_row: ActionRow,
    pub tools_field_values_suffix_stack: Stack,
    pub tools_field_values_suffix_arrow: Image,
    pub tools_field_values_spinner: Spinner,
    pub tools_weak_passwords_row: ActionRow,
    pub tools_weak_passwords_suffix_stack: Stack,
    pub tools_weak_passwords_suffix_arrow: Image,
    pub tools_weak_passwords_spinner: Spinner,
    pub tools_export_row: ActionRow,
    pub tools_export_suffix_stack: Stack,
    pub tools_export_suffix_arrow: Image,
    pub tools_export_spinner: Spinner,
    pub tools_field_values_page: NavigationPage,
    pub tools_field_values_search_entry: SearchEntry,
    pub tools_field_values_list: ListBox,
    pub tools_value_values_page: NavigationPage,
    pub tools_value_values_search_entry: SearchEntry,
    pub tools_value_values_list: ListBox,
    pub tools_weak_passwords_page: NavigationPage,
    pub tools_weak_passwords_search_entry: SearchEntry,
    pub tools_weak_passwords_list: ListBox,
}

impl EntryWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        macro_rules! required {
            ($id:literal) => {
                required_builder_object(builder, $id)?
            };
        }

        Ok(Self {
            add_button: required!("add_button"),
            find_button: required!("find_button"),
            save_button: required!("save_button"),
            password_list_filter_button: required!("password_list_filter_button"),
            password_list_filter_popover: required!("password_list_filter_popover"),
            password_list_filter_store_box: required!("password_list_filter_store_box"),
            search_entry: required!("search_entry"),
            password_list_stack: required!("password_list_stack"),
            password_list_status: required!("password_list_status"),
            password_list_spinner: required!("password_list_spinner"),
            password_list_scrolled: required!("password_list_scrolled"),
            list: required!("list"),
            password_page: required!("password_page"),
            raw_text_page: required!("raw_text_page"),
            password_status: required!("password_status"),
            password_entry: required!("password_entry"),
            password_analysis_label: required!("password_analysis_label"),
            password_generator_settings_button: required!("password_generator_settings_button"),
            password_generator_settings_revealer: required!("password_generator_settings_revealer"),
            password_generator_length_spin: required!("password_generator_length_spin"),
            password_generator_min_lowercase_spin: required!(
                "password_generator_min_lowercase_spin"
            ),
            password_generator_min_uppercase_spin: required!(
                "password_generator_min_uppercase_spin"
            ),
            password_generator_min_numbers_spin: required!("password_generator_min_numbers_spin"),
            password_generator_min_symbols_spin: required!("password_generator_min_symbols_spin"),
            username_entry: required!("username_entry"),
            otp_entry: required!("otp_entry"),
            add_field_row: required!("add_field_row"),
            apply_template_button: required!("apply_template_button"),
            clean_pass_file_button: required!("clean_pass_file_button"),
            add_otp_button: required!("add_otp_button"),
            import_private_key_button: required!("import_private_key_button"),
            editor_save_button: required!("editor_save_button"),
            copy_password_button: required!("copy_password_button"),
            qr_password_button: required!("qr_password_button"),
            copy_username_button: required!("copy_username_button"),
            qr_username_button: required!("qr_username_button"),
            copy_otp_button: required!("copy_otp_button"),
            qr_otp_button: required!("qr_otp_button"),
            text_view: required!("text_view"),
            dynamic_fields_box: required!("dynamic_fields_box"),
            open_raw_button: required!("open_raw_button"),
            tools_field_values_row: required!("tools_field_values_row"),
            tools_field_values_suffix_stack: required!("tools_field_values_suffix_stack"),
            tools_field_values_suffix_arrow: required!("tools_field_values_suffix_arrow"),
            tools_field_values_spinner: required!("tools_field_values_spinner"),
            tools_weak_passwords_row: required!("tools_weak_passwords_row"),
            tools_weak_passwords_suffix_stack: required!("tools_weak_passwords_suffix_stack"),
            tools_weak_passwords_suffix_arrow: required!("tools_weak_passwords_suffix_arrow"),
            tools_weak_passwords_spinner: required!("tools_weak_passwords_spinner"),
            tools_export_row: required!("tools_export_row"),
            tools_export_suffix_stack: required!("tools_export_suffix_stack"),
            tools_export_suffix_arrow: required!("tools_export_suffix_arrow"),
            tools_export_spinner: required!("tools_export_spinner"),
            tools_field_values_page: required!("tools_field_values_page"),
            tools_field_values_search_entry: required!("tools_field_values_search_entry"),
            tools_field_values_list: required!("tools_field_values_list"),
            tools_value_values_page: required!("tools_value_values_page"),
            tools_value_values_search_entry: required!("tools_value_values_search_entry"),
            tools_value_values_list: required!("tools_value_values_list"),
            tools_weak_passwords_page: required!("tools_weak_passwords_page"),
            tools_weak_passwords_search_entry: required!("tools_weak_passwords_search_entry"),
            tools_weak_passwords_list: required!("tools_weak_passwords_list"),
        })
    }

    pub fn connect_keyboard_navigation(&self) {
        connect_horizontal_arrow_adjustment_for_spin_buttons(&self.password_page);
    }

    pub fn configure_search_entries(&self) {
        for search_entry in [
            &self.search_entry,
            &self.tools_field_values_search_entry,
            &self.tools_value_values_search_entry,
            &self.tools_weak_passwords_search_entry,
        ] {
            configure_touch_friendly_search_entry(search_entry);
        }
    }

    pub fn toggle_find_for_visible_page(&self, navigation: &adw::NavigationView) -> bool {
        if navigation_stack_is_root(navigation) {
            toggle_password_list_search(&self.find_button, &self.search_entry, &self.list);
            return true;
        }
        for (page, search_entry) in [
            (
                &self.tools_field_values_page,
                &self.tools_field_values_search_entry,
            ),
            (
                &self.tools_value_values_page,
                &self.tools_value_values_search_entry,
            ),
            (
                &self.tools_weak_passwords_page,
                &self.tools_weak_passwords_search_entry,
            ),
        ] {
            if visible_navigation_page_is(navigation, page) {
                keycord_shell::ui::toggle_page_search_entry(&self.find_button, search_entry);
                return true;
            }
        }
        visible_navigation_page_is(navigation, &self.password_page)
            || visible_navigation_page_is(navigation, &self.raw_text_page)
    }
}

/// Focuses the preferred control for the visible Entries page.
///
/// `None` means the visible page belongs to another subject. `Some(false)` means
/// Entries owns the visible page but currently has no focusable target.
pub fn focus_visible_entry_page(
    widgets: &EntryWindowWidgets,
    navigation: &adw::NavigationView,
) -> Option<bool> {
    if navigation_stack_is_root(navigation) {
        if focus_first_password_list_row(&widgets.list) {
            return Some(true);
        }
        return Some(widgets.search_entry.is_visible() && widgets.search_entry.grab_focus());
    }
    if visible_navigation_page_is(navigation, &widgets.password_page) {
        return Some(if widgets.password_entry.is_visible() {
            widgets.password_entry.grab_focus()
        } else {
            widgets.password_page.child_focus(DirectionType::Down)
        });
    }
    if visible_navigation_page_is(navigation, &widgets.raw_text_page) {
        return Some(widgets.text_view.grab_focus());
    }
    for (page, list, search) in [
        (
            &widgets.tools_field_values_page,
            &widgets.tools_field_values_list,
            &widgets.tools_field_values_search_entry,
        ),
        (
            &widgets.tools_value_values_page,
            &widgets.tools_value_values_list,
            &widgets.tools_value_values_search_entry,
        ),
        (
            &widgets.tools_weak_passwords_page,
            &widgets.tools_weak_passwords_list,
            &widgets.tools_weak_passwords_search_entry,
        ),
    ] {
        if visible_navigation_page_is(navigation, page) {
            return Some(focus_first_keyboard_focusable_list_row(list) || search.grab_focus());
        }
    }
    None
}

/// Reports focus ownership for the visible Entries page.
pub fn visible_entry_page_contains_focus(
    widgets: &EntryWindowWidgets,
    navigation: &adw::NavigationView,
) -> Option<bool> {
    if navigation_stack_is_root(navigation) {
        return Some(
            widget_contains_focus(&widgets.list.clone().upcast())
                || widget_contains_focus(&widgets.search_entry.clone().upcast()),
        );
    }
    for page in [
        &widgets.password_page,
        &widgets.raw_text_page,
        &widgets.tools_field_values_page,
        &widgets.tools_value_values_page,
        &widgets.tools_weak_passwords_page,
    ] {
        if visible_navigation_page_is(navigation, page) {
            return Some(widget_contains_focus(&page.clone().upcast()));
        }
    }
    None
}
