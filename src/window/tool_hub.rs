//! Cross-subject navigation and wiring for the application tool hub.

mod menu;
mod widgets;

pub use widgets::ToolHubWindowWidgets;

use crate::composition::entries_ui::entry_tool_ports;
use crate::window::navigation::WindowNavigationState;
use adw::gtk::{
    Box as GtkBox, Button, Image, ListBox, ListBoxRow, MenuButton, Popover, ScrolledWindow,
    SearchEntry, Spinner, Stack,
};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, NavigationPage, PreferencesGroup, StatusPage, ToastOverlay,
};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use keycord_docs::docs_available;
use keycord_entries::ui::list::apply_password_list_store_filter;
use keycord_entries::ui::page::PasswordPageState;
use keycord_entries::ui::tools::{
    EntryToolBrowserWidgets, EntryToolsNavigation, EntryToolsState, EntryToolsWidgets,
};
use keycord_git::ui::{
    audit_tool_cache_should_clear, GitAuditPagePorts, GitAuditPageState, GitAuditPageWidgets,
    GitAuditPreferencesPorts, GitAuditWindowNavigation,
};
use keycord_preferences::Preferences;
use keycord_runtime::capabilities::supports_logging_features;
use keycord_shell::actions::{register_window_action, set_window_action_enabled};
use keycord_shell::navigation::PagePresentation;
use keycord_shell::navigation::{show_secondary_page_chrome, HasWindowChrome};
use keycord_shell::ui::{
    action_row_matches_search_query, focus_first_keyboard_focusable_list_row,
    list_box_has_visible_rows, normalized_search_query, reveal_navigation_page,
};
use keycord_stores::labels::{shortened_store_label_for_path, shortened_store_label_map};
use keycord_stores::ui::management::StoreImportToolRowState;
use keycord_stores::ui::ports::StoreUiPorts;

use self::menu::{
    append_optional_pass_import_row, append_optional_setup_row, configure_optional_doc_row,
    configure_optional_log_rows, sync_optional_setup_row,
};

const TOOLS_PAGE_TITLE: &str = "Tools";
const TOOLS_PAGE_SUBTITLE: &str = "Utilities and maintenance";

pub(crate) fn tool_hub_page_presentation() -> PagePresentation {
    PagePresentation::secondary(TOOLS_PAGE_TITLE, TOOLS_PAGE_SUBTITLE, false)
        .with_find_visible(true)
}

#[derive(Clone)]
struct ToolHubPageState {
    page: NavigationPage,
    search_entry: SearchEntry,
    primary_group: PreferencesGroup,
    empty_group: PreferencesGroup,
    list: ListBox,
    information_group: PreferencesGroup,
    logs_list: ListBox,
    docs_row: ActionRow,
    logs_row: ActionRow,
    copy_logs_row: ActionRow,
    copy_logs_button: Button,
    setup_row: Rc<RefCell<Option<ActionRow>>>,
    pass_import_row: Rc<RefCell<Option<StoreImportToolRowState>>>,
}

#[derive(Clone)]
pub struct ToolHubState {
    window: ApplicationWindow,
    navigation: WindowNavigationState,
    overlay: ToastOverlay,
    entries: EntryToolsState,
    store_ports: StoreUiPorts,
    select_page: ToolHubPageState,
    audit_page: GitAuditPageState,
}

pub struct ToolBrowserWidgets<'a> {
    pub page: &'a NavigationPage,
    pub search_entry: &'a SearchEntry,
    pub list: &'a ListBox,
}

pub struct ToolAuditWidgets<'a> {
    pub page: &'a NavigationPage,
    pub search_entry: &'a SearchEntry,
    pub stack: &'a Stack,
    pub status: &'a StatusPage,
    pub scrolled: &'a ScrolledWindow,
    pub content: &'a GtkBox,
    pub filter_button: &'a MenuButton,
    pub filter_popover: &'a Popover,
    pub filter_store_box: &'a GtkBox,
    pub filter_branch_box: &'a GtkBox,
}

pub struct ToolHubWidgets<'a> {
    pub window: &'a ApplicationWindow,
    pub navigation: &'a WindowNavigationState,
    pub page: &'a NavigationPage,
    pub search_entry: &'a SearchEntry,
    pub list: &'a ListBox,
    pub primary_group: &'a PreferencesGroup,
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
    pub audit_row: &'a ActionRow,
    pub audit_suffix_stack: &'a Stack,
    pub audit_suffix_arrow: &'a Image,
    pub audit_spinner: &'a Spinner,
    pub information_group: &'a PreferencesGroup,
    pub search_empty_group: &'a PreferencesGroup,
    pub logs_list: &'a ListBox,
    pub docs_row: &'a ActionRow,
    pub logs_row: &'a ActionRow,
    pub copy_logs_row: &'a ActionRow,
    pub copy_logs_button: &'a Button,
    pub overlay: &'a ToastOverlay,
    pub password_page: &'a PasswordPageState,
    pub store_ports: &'a StoreUiPorts,
    pub field_values: ToolBrowserWidgets<'a>,
    pub value_values: ToolBrowserWidgets<'a>,
    pub weak_passwords: ToolBrowserWidgets<'a>,
    pub audit: ToolAuditWidgets<'a>,
    pub root_list: &'a ListBox,
    pub root_search_entry: &'a SearchEntry,
}

impl ToolHubState {
    pub fn new(widgets: ToolHubWidgets<'_>) -> Self {
        let entry_filter_list = widgets.root_list.clone();
        let select_page = ToolHubPageState {
            page: widgets.page.clone(),
            search_entry: widgets.search_entry.clone(),
            primary_group: widgets.primary_group.clone(),
            empty_group: widgets.search_empty_group.clone(),
            list: widgets.list.clone(),
            information_group: widgets.information_group.clone(),
            logs_list: widgets.logs_list.clone(),
            docs_row: widgets.docs_row.clone(),
            logs_row: widgets.logs_row.clone(),
            copy_logs_row: widgets.copy_logs_row.clone(),
            copy_logs_button: widgets.copy_logs_button.clone(),
            setup_row: Rc::new(RefCell::new(None)),
            pass_import_row: Rc::new(RefCell::new(None)),
        };
        let audit_page = GitAuditPageState::new(
            GitAuditPageWidgets {
                navigation: GitAuditWindowNavigation {
                    nav: widgets.navigation.nav.clone(),
                    back: widgets.navigation.back.clone(),
                    add: widgets.navigation.add.clone(),
                    find: widgets.navigation.find.clone(),
                    primary_action: widgets.navigation.primary_action.clone(),
                    secondary_action: widgets.navigation.secondary_action.clone(),
                    save: widgets.navigation.save.clone(),
                    raw: widgets.navigation.raw.clone(),
                    title: widgets.navigation.title.clone(),
                },
                overlay: widgets.overlay,
                tool_row: widgets.audit_row,
                tool_suffix_stack: widgets.audit_suffix_stack,
                tool_suffix_arrow: widgets.audit_suffix_arrow,
                tool_spinner: widgets.audit_spinner,
                page: widgets.audit.page,
                search_entry: widgets.audit.search_entry,
                stack: widgets.audit.stack,
                status: widgets.audit.status,
                scrolled: widgets.audit.scrolled,
                content: widgets.audit.content,
                filter_button: widgets.audit.filter_button,
                filter_popover: widgets.audit.filter_popover,
                filter_store_box: widgets.audit.filter_store_box,
                filter_branch_box: widgets.audit.filter_branch_box,
            },
            GitAuditPagePorts {
                preferences: GitAuditPreferencesPorts {
                    store_roots: Rc::new(|| Preferences::new().store_roots()),
                    included_store_roots: Rc::new(|| {
                        Preferences::new().filter_included_store_roots()
                    }),
                    set_included_store_roots: Rc::new(|roots| {
                        Preferences::new()
                            .set_filter_included_store_roots(roots)
                            .map_err(|err| err.to_string())
                    }),
                    included_branches: Rc::new(|| {
                        Preferences::new().audit_filter_included_branches()
                    }),
                    set_included_branches: Rc::new(|branches| {
                        Preferences::new()
                            .set_audit_filter_included_branches(branches)
                            .map_err(|err| err.to_string())
                    }),
                    use_commit_history_recipients: Rc::new(|| {
                        Preferences::new().audit_use_commit_history_recipients()
                    }),
                },
                load_commit_page: Arc::new(|store_root, full_ref, use_history, page| {
                    crate::composition::git_audit::load_store_git_audit_commit_page(
                        &store_root,
                        &full_ref,
                        use_history,
                        page,
                    )
                }),
                store_label_map: Rc::new(shortened_store_label_map),
                store_label_for_path: Rc::new(shortened_store_label_for_path),
                apply_entry_store_filter: Rc::new(move |included| {
                    apply_password_list_store_filter(&entry_filter_list, included);
                }),
            },
        );
        let entry_navigation = EntryToolsNavigation {
            nav: widgets.navigation.nav.clone(),
            back: widgets.navigation.back.clone(),
            add: widgets.navigation.add.clone(),
            find: widgets.navigation.find.clone(),
            primary_action: widgets.navigation.primary_action.clone(),
            secondary_action: widgets.navigation.secondary_action.clone(),
            save: widgets.navigation.save.clone(),
            raw: widgets.navigation.raw.clone(),
            title: widgets.navigation.title.clone(),
        };
        let select_page_for_entries = select_page.clone();
        let entries = EntryToolsState::new(
            EntryToolsWidgets {
                window: widgets.window,
                navigation: &entry_navigation,
                overlay: widgets.overlay,
                password_page: widgets.password_page,
                tools_page: widgets.page,
                root_list: widgets.root_list,
                root_search_entry: widgets.root_search_entry,
                field_values_row: widgets.field_values_row,
                field_values_suffix_stack: widgets.field_values_suffix_stack,
                field_values_suffix_arrow: widgets.field_values_suffix_arrow,
                field_values_spinner: widgets.field_values_spinner,
                weak_passwords_row: widgets.weak_passwords_row,
                weak_passwords_suffix_stack: widgets.weak_passwords_suffix_stack,
                weak_passwords_suffix_arrow: widgets.weak_passwords_suffix_arrow,
                weak_passwords_spinner: widgets.weak_passwords_spinner,
                export_row: widgets.export_row,
                export_suffix_stack: widgets.export_suffix_stack,
                export_suffix_arrow: widgets.export_suffix_arrow,
                export_spinner: widgets.export_spinner,
                field_values: EntryToolBrowserWidgets {
                    page: widgets.field_values.page,
                    search_entry: widgets.field_values.search_entry,
                    list: widgets.field_values.list,
                },
                value_values: EntryToolBrowserWidgets {
                    page: widgets.value_values.page,
                    search_entry: widgets.value_values.search_entry,
                    list: widgets.value_values.list,
                },
                weak_passwords: EntryToolBrowserWidgets {
                    page: widgets.weak_passwords.page,
                    search_entry: widgets.weak_passwords.search_entry,
                    list: widgets.weak_passwords.list,
                },
            },
            entry_tool_ports(Rc::new(move || {
                render_select_page_search_results_for(&select_page_for_entries);
            })),
        );
        let state = Self {
            window: widgets.window.clone(),
            navigation: widgets.navigation.clone(),
            overlay: widgets.overlay.clone(),
            entries,
            store_ports: widgets.store_ports.clone(),
            select_page,
            audit_page,
        };
        state.initialize_select_page();
        state.connect_select_page_handlers();
        state.connect_browser_handlers();
        state
    }

    fn initialize_select_page(&self) {
        configure_optional_doc_row(self);
        configure_optional_log_rows(self);
        *self.select_page.setup_row.borrow_mut() = append_optional_setup_row(self);
        *self.select_page.pass_import_row.borrow_mut() = append_optional_pass_import_row(self);
        self.sync_action_availability();
        self.sync_tool_rows();
        sync_optional_setup_row(self.select_page.setup_row.borrow().as_ref());
    }

    fn close_select_dialog(&self) {}

    pub fn refresh_select_page(&self) {
        self.audit_page.clear_transient_state();
        self.entries.refresh();
        self.sync_action_availability();
        self.sync_tool_rows();
        sync_optional_setup_row(self.select_page.setup_row.borrow().as_ref());
        if let Some(pass_import_row) = self.select_page.pass_import_row.borrow().as_ref() {
            pass_import_row.refresh();
        }
        self.render_select_page_search_results();
    }

    fn connect_select_page_handlers(&self) {
        let search_entry = self.select_page.search_entry.clone();
        self.select_page
            .list
            .set_filter_func(move |row| tool_search_matches_list_row(row, &search_entry));

        let search_entry = self.select_page.search_entry.clone();
        self.select_page
            .logs_list
            .set_filter_func(move |row| tool_search_matches_list_row(row, &search_entry));

        let state = self.clone();
        self.select_page
            .search_entry
            .connect_search_changed(move |_| state.render_select_page_search_results());
    }

    fn connect_browser_handlers(&self) {
        {
            let state = self.clone();
            self.navigation
                .nav
                .connect_notify_local(Some("visible-page"), move |_, _| {
                    state.handle_navigation_visibility_change();
                });
        }
    }

    fn handle_navigation_visibility_change(&self) {
        let audit_page_visible = self.audit_page.is_visible();
        let audit_page_in_stack = self.audit_page.is_in_stack();
        self.audit_page.sync_filter_button();
        if audit_tool_cache_should_clear(audit_page_visible, audit_page_in_stack)
            && self.audit_page.has_transient_state()
        {
            self.audit_page.clear_transient_state();
        }

        self.entries
            .handle_navigation_visibility_change(audit_page_visible);
    }

    fn sync_tool_rows(&self) {
        self.entries.sync_tool_rows();
        self.audit_page.sync_tool_row();
        self.render_select_page_search_results();
    }

    pub(crate) fn render_audit_page(&self) {
        self.audit_page.render_audit_page();
    }

    pub fn sync_action_availability(&self) {
        sync_tools_action_availability(&self.window);
    }

    pub fn open(&self) {
        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(&chrome, TOOLS_PAGE_TITLE, TOOLS_PAGE_SUBTITLE, false);
        chrome.find.set_visible(true);
        self.refresh_select_page();
        reveal_navigation_page(&self.navigation.nav, &self.select_page.page);
        if self.select_page.search_entry.is_visible() {
            let _ = self.select_page.search_entry.grab_focus();
            return;
        }
        let _ = focus_first_keyboard_focusable_list_row(&self.select_page.list)
            || focus_first_keyboard_focusable_list_row(&self.select_page.logs_list);
    }

    fn render_select_page_search_results(&self) {
        render_select_page_search_results_for(&self.select_page);
    }
}

fn render_select_page_search_results_for(select_page: &ToolHubPageState) {
    select_page.list.invalidate_filter();
    select_page.logs_list.invalidate_filter();

    let tools_visible = list_box_has_visible_rows(&select_page.list);
    let information_visible = (docs_available() || supports_logging_features())
        && list_box_has_visible_rows(&select_page.logs_list);
    let searching = !normalized_search_query(select_page.search_entry.text().as_str()).is_empty();

    select_page.primary_group.set_visible(tools_visible);
    select_page
        .information_group
        .set_visible(information_visible);
    select_page
        .empty_group
        .set_visible(searching && !tools_visible && !information_visible);
}

fn tool_search_matches_list_row(row: &ListBoxRow, search_entry: &SearchEntry) -> bool {
    action_row_matches_search_query(row, search_entry.text().as_str())
}

pub fn sync_tools_action_availability(window: &ApplicationWindow) {
    let _ = set_window_action_enabled(window, "open-tools", true);
}

pub fn register_open_tools_action(window: &ApplicationWindow, open_tools: impl Fn() + 'static) {
    register_window_action(window, "open-tools", open_tools);
    sync_tools_action_availability(window);
}
