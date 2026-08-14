//! Entry-specific tool browsers and export presentation.

mod export;
mod field_values;
mod ports;
#[cfg(test)]
mod tests;
mod unlock;
mod weak_passwords;

pub use self::ports::*;

use std::cell::Cell;
use std::rc::Rc;

use adw::gtk::{Button, Image, ListBox, ListBoxRow, SearchEntry, Spinner, Stack};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, NavigationPage, NavigationView, ToastOverlay, WindowTitle,
};
use keycord_runtime::i18n::gettext;
use keycord_shell::navigation::{
    HasWindowChrome, NavigationPageId, NavigationPageRoute, PagePresentation, WindowChrome,
};
use keycord_shell::object_data::non_null_to_string_option;
use keycord_shell::ui::{
    append_info_row, append_spinner_row, connect_keyboard_focusable_search_list_arrow_navigation,
    set_action_row_enabled, set_action_row_suffix_loading, visible_navigation_page_is,
};

use super::list::password_list_render_generation;
use super::page::PasswordPageState;
use super::widgets::EntryWindowWidgets;
use crate::tools::EntryRequest as FieldValueRequest;

use self::field_values::FieldValueBrowserState;
use self::weak_passwords::WeakPasswordToolState;

const FIELD_VALUES_TITLE: &str = "Browse field values";
const FIELD_VALUES_FIELDS_SUBTITLE: &str = "Pick a field from the current list.";
const FIELD_VALUES_VALUES_SUBTITLE: &str = "Pick a value from the current list.";
const FIELD_VALUES_ROW_SUBTITLE: &str = "Browse unique field values from the current list.";
const FIELD_VALUES_LOADING_TITLE: &str = "Loading field values";
const FIELD_VALUES_LOADING_SUBTITLE: &str = "Reading searchable pass fields from the current list.";
const FIELD_VALUES_EMPTY_TITLE: &str = "No searchable fields";
const FIELD_VALUES_EMPTY_SUBTITLE: &str =
    "The current list doesn't have any searchable pass fields.";
const FIELD_VALUES_FILTER_EMPTY_TITLE: &str = "No matching fields";
const FIELD_VALUES_FILTER_EMPTY_SUBTITLE: &str = "Try a different field filter.";
const VALUE_VALUES_EMPTY_TITLE: &str = "No values";
const VALUE_VALUES_EMPTY_SUBTITLE: &str = "This field has no searchable values.";
const VALUE_VALUES_FILTER_EMPTY_TITLE: &str = "No matching values";
const VALUE_VALUES_FILTER_EMPTY_SUBTITLE: &str = "Try a different value filter.";
const WEAK_PASSWORDS_TITLE: &str = "Find weak passwords";
const WEAK_PASSWORDS_SUBTITLE: &str = "Scan the current list for passwords that fail basic checks.";
const WEAK_PASSWORDS_ROW_SUBTITLE: &str =
    "Scan the current list for passwords that fail basic checks.";
const WEAK_PASSWORDS_LOADING_TITLE: &str = "Scanning passwords";
const WEAK_PASSWORDS_LOADING_SUBTITLE: &str = "Reading password lines from the current list.";
const WEAK_PASSWORDS_EMPTY_TITLE: &str = "No weak passwords found";
const WEAK_PASSWORDS_EMPTY_SUBTITLE: &str =
    "No loaded pass files matched the current weak-password checks.";
const WEAK_PASSWORDS_FILTER_EMPTY_TITLE: &str = "No matching results";
const WEAK_PASSWORDS_FILTER_EMPTY_SUBTITLE: &str = "Try a different search term.";
const EXPORT_ROW_SUBTITLE: &str = "Export every password and field to a CSV file.";

pub const FIELD_VALUES_PAGE_ID: NavigationPageId = NavigationPageId::new("tool-field-values");
pub const VALUE_VALUES_PAGE_ID: NavigationPageId = NavigationPageId::new("tool-value-values");
pub const WEAK_PASSWORDS_PAGE_ID: NavigationPageId = NavigationPageId::new("tool-weak-passwords");

pub fn entry_tool_navigation_routes(widgets: &EntryWindowWidgets) -> [NavigationPageRoute; 3] {
    [
        NavigationPageRoute::secondary(
            FIELD_VALUES_PAGE_ID,
            &widgets.tools_field_values_page,
            PagePresentation::secondary(FIELD_VALUES_TITLE, FIELD_VALUES_FIELDS_SUBTITLE, false)
                .with_find_visible(true),
        ),
        NavigationPageRoute::secondary(
            VALUE_VALUES_PAGE_ID,
            &widgets.tools_value_values_page,
            PagePresentation::secondary(FIELD_VALUES_TITLE, FIELD_VALUES_VALUES_SUBTITLE, false)
                .with_find_visible(true),
        ),
        NavigationPageRoute::secondary(
            WEAK_PASSWORDS_PAGE_ID,
            &widgets.tools_weak_passwords_page,
            PagePresentation::secondary(WEAK_PASSWORDS_TITLE, WEAK_PASSWORDS_SUBTITLE, false)
                .with_find_visible(true),
        ),
    ]
}

#[derive(Clone)]
pub struct EntryToolsNavigation {
    pub nav: NavigationView,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub primary_action: Button,
    pub secondary_action: Button,
    pub save: Button,
    pub raw: Button,
    pub title: WindowTitle,
}

impl HasWindowChrome for EntryToolsNavigation {
    fn window_chrome(&self) -> WindowChrome<'_> {
        WindowChrome {
            back: &self.back,
            add: &self.add,
            find: &self.find,
            primary_action: &self.primary_action,
            secondary_action: &self.secondary_action,
            save: &self.save,
            raw: &self.raw,
            title: &self.title,
        }
    }
}

pub struct EntryToolBrowserWidgets<'a> {
    pub page: &'a NavigationPage,
    pub search_entry: &'a SearchEntry,
    pub list: &'a ListBox,
}

pub struct EntryToolsWidgets<'a> {
    pub window: &'a ApplicationWindow,
    pub navigation: &'a EntryToolsNavigation,
    pub overlay: &'a ToastOverlay,
    pub password_page: &'a PasswordPageState,
    pub tools_page: &'a NavigationPage,
    pub root_list: &'a ListBox,
    pub root_search_entry: &'a SearchEntry,
    pub field_values_row: &'a ActionRow,
    pub field_values_suffix_stack: &'a Stack,
    pub field_values_suffix_arrow: &'a Image,
    pub field_values_spinner: &'a Spinner,
    pub weak_passwords_row: &'a ActionRow,
    pub weak_passwords_suffix_stack: &'a Stack,
    pub weak_passwords_suffix_arrow: &'a Image,
    pub weak_passwords_spinner: &'a Spinner,
    pub export_row: &'a ActionRow,
    pub export_suffix_stack: &'a Stack,
    pub export_suffix_arrow: &'a Image,
    pub export_spinner: &'a Spinner,
    pub field_values: EntryToolBrowserWidgets<'a>,
    pub value_values: EntryToolBrowserWidgets<'a>,
    pub weak_passwords: EntryToolBrowserWidgets<'a>,
}

#[derive(Clone)]
struct EntryToolSelectState {
    page: NavigationPage,
    field_values_row: ActionRow,
    field_values_suffix_stack: Stack,
    field_values_suffix_arrow: Image,
    field_values_spinner: Spinner,
    weak_passwords_row: ActionRow,
    weak_passwords_suffix_stack: Stack,
    weak_passwords_suffix_arrow: Image,
    weak_passwords_spinner: Spinner,
    export_row: ActionRow,
    export_suffix_stack: Stack,
    export_suffix_arrow: Image,
    export_spinner: Spinner,
    export_busy: Rc<Cell<bool>>,
}

#[derive(Clone)]
struct ToolFieldBrowserPageState {
    field_page: NavigationPage,
    field_search_entry: SearchEntry,
    field_list: ListBox,
    value_page: NavigationPage,
    value_search_entry: SearchEntry,
    value_list: ListBox,
    browser: Rc<FieldValueBrowserState>,
}

#[derive(Clone)]
struct ToolWeakPasswordPageState {
    page: NavigationPage,
    search_entry: SearchEntry,
    list: ListBox,
    weak_passwords: Rc<WeakPasswordToolState>,
}

#[derive(Clone)]
pub struct EntryToolsState {
    window: ApplicationWindow,
    navigation: EntryToolsNavigation,
    overlay: ToastOverlay,
    password_page: PasswordPageState,
    root_list: ListBox,
    root_search_entry: SearchEntry,
    select_page: EntryToolSelectState,
    field_browser: ToolFieldBrowserPageState,
    weak_password_page: ToolWeakPasswordPageState,
    ports: EntryToolUiPorts,
}

impl EntryToolsState {
    pub fn new(widgets: EntryToolsWidgets<'_>, ports: EntryToolUiPorts) -> Self {
        let state = Self {
            window: widgets.window.clone(),
            navigation: widgets.navigation.clone(),
            overlay: widgets.overlay.clone(),
            password_page: widgets.password_page.clone(),
            root_list: widgets.root_list.clone(),
            root_search_entry: widgets.root_search_entry.clone(),
            select_page: EntryToolSelectState {
                page: widgets.tools_page.clone(),
                field_values_row: widgets.field_values_row.clone(),
                field_values_suffix_stack: widgets.field_values_suffix_stack.clone(),
                field_values_suffix_arrow: widgets.field_values_suffix_arrow.clone(),
                field_values_spinner: widgets.field_values_spinner.clone(),
                weak_passwords_row: widgets.weak_passwords_row.clone(),
                weak_passwords_suffix_stack: widgets.weak_passwords_suffix_stack.clone(),
                weak_passwords_suffix_arrow: widgets.weak_passwords_suffix_arrow.clone(),
                weak_passwords_spinner: widgets.weak_passwords_spinner.clone(),
                export_row: widgets.export_row.clone(),
                export_suffix_stack: widgets.export_suffix_stack.clone(),
                export_suffix_arrow: widgets.export_suffix_arrow.clone(),
                export_spinner: widgets.export_spinner.clone(),
                export_busy: Rc::new(Cell::new(false)),
            },
            field_browser: ToolFieldBrowserPageState {
                field_page: widgets.field_values.page.clone(),
                field_search_entry: widgets.field_values.search_entry.clone(),
                field_list: widgets.field_values.list.clone(),
                value_page: widgets.value_values.page.clone(),
                value_search_entry: widgets.value_values.search_entry.clone(),
                value_list: widgets.value_values.list.clone(),
                browser: Rc::new(FieldValueBrowserState::default()),
            },
            weak_password_page: ToolWeakPasswordPageState {
                page: widgets.weak_passwords.page.clone(),
                search_entry: widgets.weak_passwords.search_entry.clone(),
                list: widgets.weak_passwords.list.clone(),
                weak_passwords: Rc::new(WeakPasswordToolState::default()),
            },
            ports,
        };
        state.connect_handlers();
        state.sync_tool_rows();
        state
    }

    fn connect_handlers(&self) {
        let state = self.clone();
        self.select_page
            .field_values_row
            .connect_activated(move |_| state.prepare_field_values_browser());

        let state = self.clone();
        self.select_page
            .weak_passwords_row
            .connect_activated(move |_| state.prepare_weak_passwords_browser());

        self.connect_export_tool();

        {
            let state = self.clone();
            self.field_browser
                .field_search_entry
                .connect_search_changed(move |_| state.render_field_list());
        }
        {
            let state = self.clone();
            self.field_browser
                .value_search_entry
                .connect_search_changed(move |_| state.render_value_list());
        }
        {
            let state = self.clone();
            self.weak_password_page
                .search_entry
                .connect_search_changed(move |_| state.render_weak_passwords_list());
        }

        connect_keyboard_focusable_search_list_arrow_navigation(
            &self.field_browser.field_list,
            &self.field_browser.field_search_entry,
        );
        connect_keyboard_focusable_search_list_arrow_navigation(
            &self.field_browser.value_list,
            &self.field_browser.value_search_entry,
        );
        connect_keyboard_focusable_search_list_arrow_navigation(
            &self.weak_password_page.list,
            &self.weak_password_page.search_entry,
        );
    }

    pub fn refresh(&self) {
        self.invalidate_stale_tool_cache();
        self.sync_tool_rows();
    }

    fn close_select_dialog(&self) {}

    pub fn handle_navigation_visibility_change(&self, audit_page_visible: bool) {
        if visible_navigation_page_is(&self.navigation.nav, &self.weak_password_page.page) {
            self.refresh_weak_passwords_browser_if_needed();
            return;
        }
        if self.browser_flow_is_visible(audit_page_visible) {
            return;
        }
        self.reset_field_values_view();
        self.clear_weak_passwords_cache();
        self.invalidate_stale_tool_cache();
    }

    pub fn browser_flow_is_visible(&self, audit_page_visible: bool) -> bool {
        tool_browser_flow_is_visible(
            visible_navigation_page_is(&self.navigation.nav, &self.select_page.page),
            visible_navigation_page_is(&self.navigation.nav, &self.field_browser.field_page),
            visible_navigation_page_is(&self.navigation.nav, &self.field_browser.value_page),
            visible_navigation_page_is(&self.navigation.nav, &self.weak_password_page.page),
            audit_page_visible,
            visible_navigation_page_is(&self.navigation.nav, &self.password_page.page),
            visible_navigation_page_is(&self.navigation.nav, &self.password_page.raw_page),
        )
    }

    fn current_password_list_generation(&self) -> Option<u64> {
        password_list_render_generation(&self.root_list)
    }

    fn field_values_cache_is_current(&self, generation: Option<u64>) -> bool {
        self.field_browser.browser.source_generation.get() == generation
            && self.field_browser.browser.catalog.borrow().is_some()
    }

    fn invalidate_stale_tool_cache(&self) {
        let generation = self.current_password_list_generation();
        if self.field_browser.browser.source_generation.get() != generation {
            self.clear_field_values_cache();
        }
    }

    fn set_field_values_tool_busy(&self, busy: bool) {
        self.field_browser.browser.tool_busy.set(busy);
        self.sync_tool_rows();
    }

    fn set_weak_passwords_tool_busy(&self, busy: bool) {
        self.weak_password_page.weak_passwords.tool_busy.set(busy);
        self.sync_tool_rows();
    }

    fn set_export_tool_busy(&self, busy: bool) {
        self.select_page.export_busy.set(busy);
        self.sync_tool_rows();
    }

    fn advanced_search_tools_are_busy(&self) -> bool {
        self.field_browser.browser.tool_busy.get()
            || self.weak_password_page.weak_passwords.tool_busy.get()
            || self.select_page.export_busy.get()
    }

    pub fn sync_tool_rows(&self) {
        let advanced_search_enabled = advanced_search_tool_rows_enabled(
            self.field_browser.browser.tool_busy.get(),
            self.weak_password_page.weak_passwords.tool_busy.get(),
        ) && !self.select_page.export_busy.get();
        set_tool_action_row_state(
            &self.select_page.field_values_row,
            &self.select_page.field_values_suffix_stack,
            &self.select_page.field_values_suffix_arrow,
            &self.select_page.field_values_spinner,
            advanced_search_enabled,
            self.field_browser.browser.tool_busy.get(),
            FIELD_VALUES_ROW_SUBTITLE,
        );
        set_tool_action_row_state(
            &self.select_page.weak_passwords_row,
            &self.select_page.weak_passwords_suffix_stack,
            &self.select_page.weak_passwords_suffix_arrow,
            &self.select_page.weak_passwords_spinner,
            advanced_search_enabled,
            self.weak_password_page.weak_passwords.tool_busy.get(),
            WEAK_PASSWORDS_ROW_SUBTITLE,
        );
        set_tool_action_row_state(
            &self.select_page.export_row,
            &self.select_page.export_suffix_stack,
            &self.select_page.export_suffix_arrow,
            &self.select_page.export_spinner,
            advanced_search_enabled,
            self.select_page.export_busy.get(),
            EXPORT_ROW_SUBTITLE,
        );
        (self.ports.refresh_tool_hub)();
    }
}

fn collect_loaded_entry_requests(list: &ListBox) -> Vec<FieldValueRequest> {
    let mut requests = Vec::new();
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        let Ok(row) = widget.downcast::<ListBoxRow>() else {
            child = next;
            continue;
        };
        let Some(root) = non_null_to_string_option(&row, "root") else {
            child = next;
            continue;
        };
        let Some(label) = non_null_to_string_option(&row, "label") else {
            child = next;
            continue;
        };
        requests.push(FieldValueRequest { root, label });
        child = next;
    }

    requests
}

fn append_loading_rows(list: &ListBox, title: &str, subtitle: &str) {
    append_info_row(list, title, subtitle);
    append_spinner_row(list);
}

fn advanced_search_tool_rows_enabled(field_values_busy: bool, weak_passwords_busy: bool) -> bool {
    !(field_values_busy || weak_passwords_busy)
}

const fn tool_browser_flow_is_visible(
    tools_page_visible: bool,
    field_values_page_visible: bool,
    value_values_page_visible: bool,
    weak_passwords_page_visible: bool,
    audit_page_visible: bool,
    password_page_visible: bool,
    raw_password_page_visible: bool,
) -> bool {
    tools_page_visible
        || field_values_page_visible
        || value_values_page_visible
        || weak_passwords_page_visible
        || audit_page_visible
        || password_page_visible
        || raw_password_page_visible
}

fn set_tool_action_row_state(
    row: &ActionRow,
    suffix_stack: &Stack,
    suffix_arrow: &Image,
    spinner: &Spinner,
    enabled: bool,
    loading: bool,
    subtitle: &str,
) {
    row.set_subtitle(&gettext(subtitle));
    set_action_row_suffix_loading(suffix_stack, suffix_arrow, spinner, loading);
    set_action_row_enabled(row, enabled);
}
