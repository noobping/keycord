use super::widgets::WindowWidgets;
use crate::composition::entries_ui::{entry_page_ports, execute_undo_action, reload_password_list};
use crate::composition::preferences_ui::PreferencesActionState;
use crate::composition::stores_ui::{key_management_ui_ports, store_ui_ports};
use crate::window::controls::{BackActionState, PlatformBackActionState};
use crate::window::navigation::{restore_window_for_current_page, WindowNavigationState};
use crate::window::tool_hub::{ToolAuditWidgets, ToolBrowserWidgets, ToolHubState, ToolHubWidgets};
use adw::prelude::*;
use keycord_docs::{DocumentationNavigation, DocumentationPageState};
use keycord_entries::generation::PasswordGenerationControls;
use keycord_entries::otp::PasswordOtpState;
use keycord_entries::ui::list::{PasswordListActions, PasswordListVisibilityState};
use keycord_entries::ui::new_item::NewPasswordDialogState;
use keycord_entries::ui::page::PasswordPageState;
use keycord_entries::ui::undo::{ContextUndoActionState, ContextUndoPorts};
use keycord_git::ui::{GitActionPorts, GitActionState, StoreGitPagePorts, StoreGitPageState};
use keycord_keys::ui::{KeyManagementUiParts, KeyManagementUiState};
use keycord_preferences::Preferences;
use keycord_shell::navigation::{
    show_secondary_page_chrome, HasWindowChrome, WindowChrome, WindowPageState,
};
use keycord_stores::ui::management::StoreRecipientsPageState;
use keycord_stores::ui::ports::StoreNavigationUiPorts;
use std::rc::Rc;
use std::sync::Arc;

pub(super) fn new_password_dialog_state(_widgets: &WindowWidgets) -> NewPasswordDialogState {
    NewPasswordDialogState::new(
        || Preferences::new().store_roots(),
        || Preferences::new().filter_included_store_roots(),
    )
}

pub(super) fn password_page_state(
    widgets: &WindowWidgets,
    otp: &PasswordOtpState,
) -> PasswordPageState {
    PasswordPageState::new(
        &widgets.entries,
        window_page_state(widgets, &widgets.entries.password_page),
        otp,
        &widgets.shell.overlay,
        entry_page_ports(),
    )
}

pub(super) fn store_git_page_state(widgets: &WindowWidgets) -> StoreGitPageState {
    let busy_window = widgets.shell.window.clone();
    let refresh_window = widgets.shell.window.clone();
    StoreGitPageState::new(
        &widgets.git,
        window_page_state(widgets, &widgets.git.store_git_page),
        &widgets.shell.overlay,
        StoreGitPagePorts {
            append_optional_host_access_row: Rc::new(
                crate::composition::host_access::append_optional_host_access_group_row,
            ),
            set_application_busy: Rc::new(move |busy| {
                crate::composition::git_ui::set_application_busy(&busy_window, busy);
            }),
            refresh_related_views: Rc::new(move || {
                crate::composition::git_ui::refresh_related_views(&refresh_window);
            }),
        },
    )
}

fn build_key_management_state(widgets: &WindowWidgets) -> KeyManagementUiState {
    KeyManagementUiState::new(KeyManagementUiParts {
        window: widgets.shell.window.clone(),
        navigation: widgets.shell.navigation.clone(),
        overlay: widgets.shell.overlay.clone(),
        widgets: widgets.keys.clone(),
        #[cfg(feature = "fidokey")]
        fido: widgets.fido.clone(),
        ports: key_management_ui_ports(),
    })
}

fn build_store_recipients_page_state(
    widgets: &WindowWidgets,
    store_git_page: &StoreGitPageState,
) -> StoreRecipientsPageState {
    let key_management = build_key_management_state(widgets);

    let back = widgets.shell.back.clone();
    let add = widgets.entries.add_button.clone();
    let find = widgets.entries.find_button.clone();
    let git = widgets.git.git_button.clone();
    let store = widgets.stores.store_button.clone();
    let save = widgets.entries.save_button.clone();
    let raw = widgets.entries.open_raw_button.clone();
    let title = widgets.shell.title.clone();
    let navigation = StoreNavigationUiPorts {
        show_secondary_page: Rc::new(move |page_title, subtitle, save_visible| {
            let chrome = WindowChrome {
                back: &back,
                add: &add,
                find: &find,
                primary_action: &git,
                secondary_action: &store,
                save: &save,
                raw: &raw,
                title: &title,
            };
            show_secondary_page_chrome(&chrome, page_title, subtitle, save_visible);
        }),
    };

    let state = StoreRecipientsPageState::new(
        &widgets.stores,
        window_page_state(widgets, &widgets.stores.recipients_page),
        &widgets.shell.overlay,
        key_management,
        store_ui_ports(store_git_page, navigation),
    );
    *store_git_page.recipients_page.borrow_mut() = Some(state.clone());
    state
}

pub(super) fn store_recipients_page_state(
    widgets: &WindowWidgets,
    store_git_page: &StoreGitPageState,
) -> StoreRecipientsPageState {
    build_store_recipients_page_state(widgets, store_git_page)
}

pub(super) fn window_navigation_state(widgets: &WindowWidgets) -> WindowNavigationState {
    WindowNavigationState {
        nav: widgets.shell.navigation.clone(),
        entries: widgets.entries.clone(),
        keys: widgets.keys.clone(),
        settings_page: widgets.preferences.page.clone(),
        tools_page: widgets.tool_hub.page.clone(),
        docs_page: widgets.docs.page.clone(),
        docs_detail_page: widgets.docs.detail_page.clone(),
        tools_audit_page: widgets.git.tools_audit_page.clone(),
        store_import_page: widgets.stores.import_page.clone(),
        log_page: widgets.shell.log_page.clone(),
        back: widgets.shell.back.clone(),
        add: widgets.entries.add_button.clone(),
        find: widgets.entries.find_button.clone(),
        primary_action: widgets.git.git_button.clone(),
        secondary_action: widgets.stores.store_button.clone(),
        save: widgets.entries.save_button.clone(),
        raw: widgets.entries.open_raw_button.clone(),
        title: widgets.shell.title.clone(),
    }
}

pub(super) fn docs_page_state(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
) -> DocumentationPageState {
    let navigation_for_chrome = navigation.clone();
    let docs_navigation = DocumentationNavigation::new(
        &navigation.nav,
        &navigation.docs_page,
        move |title, subtitle, find_visible| {
            let chrome = navigation_for_chrome.window_chrome();
            show_secondary_page_chrome(&chrome, title, subtitle, false);
            chrome.find.set_visible(find_visible);
        },
    );
    widgets.docs.page_state(docs_navigation)
}

pub(super) fn tool_hub_state(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
    password_page: &PasswordPageState,
    store_ports: &keycord_stores::ui::ports::StoreUiPorts,
) -> ToolHubState {
    ToolHubState::new(ToolHubWidgets {
        window: &widgets.shell.window,
        navigation,
        page: &widgets.tool_hub.page,
        search_entry: &widgets.tool_hub.search_entry,
        list: &widgets.tool_hub.primary_list,
        primary_group: &widgets.tool_hub.primary_group,
        field_values_row: &widgets.entries.tools_field_values_row,
        field_values_suffix_stack: &widgets.entries.tools_field_values_suffix_stack,
        field_values_suffix_arrow: &widgets.entries.tools_field_values_suffix_arrow,
        field_values_spinner: &widgets.entries.tools_field_values_spinner,
        weak_passwords_row: &widgets.entries.tools_weak_passwords_row,
        weak_passwords_suffix_stack: &widgets.entries.tools_weak_passwords_suffix_stack,
        weak_passwords_suffix_arrow: &widgets.entries.tools_weak_passwords_suffix_arrow,
        weak_passwords_spinner: &widgets.entries.tools_weak_passwords_spinner,
        export_row: &widgets.entries.tools_export_row,
        export_suffix_stack: &widgets.entries.tools_export_suffix_stack,
        export_suffix_arrow: &widgets.entries.tools_export_suffix_arrow,
        export_spinner: &widgets.entries.tools_export_spinner,
        audit_row: &widgets.git.tools_audit_row,
        audit_suffix_stack: &widgets.git.tools_audit_suffix_stack,
        audit_suffix_arrow: &widgets.git.tools_audit_suffix_arrow,
        audit_spinner: &widgets.git.tools_audit_spinner,
        information_group: &widgets.tool_hub.information_group,
        search_empty_group: &widgets.tool_hub.search_empty_group,
        logs_list: &widgets.tool_hub.information_list,
        docs_row: &widgets.docs.tool_row,
        logs_row: &widgets.shell.log_tool_row,
        copy_logs_row: &widgets.shell.copy_logs_tool_row,
        copy_logs_button: &widgets.shell.copy_logs_button,
        overlay: &widgets.shell.overlay,
        password_page,
        store_ports,
        field_values: ToolBrowserWidgets {
            page: &widgets.entries.tools_field_values_page,
            search_entry: &widgets.entries.tools_field_values_search_entry,
            list: &widgets.entries.tools_field_values_list,
        },
        value_values: ToolBrowserWidgets {
            page: &widgets.entries.tools_value_values_page,
            search_entry: &widgets.entries.tools_value_values_search_entry,
            list: &widgets.entries.tools_value_values_list,
        },
        weak_passwords: ToolBrowserWidgets {
            page: &widgets.entries.tools_weak_passwords_page,
            search_entry: &widgets.entries.tools_weak_passwords_search_entry,
            list: &widgets.entries.tools_weak_passwords_list,
        },
        audit: ToolAuditWidgets {
            page: &widgets.git.tools_audit_page,
            search_entry: &widgets.git.tools_audit_search_entry,
            stack: &widgets.git.tools_audit_stack,
            status: &widgets.git.tools_audit_status,
            scrolled: &widgets.git.tools_audit_scrolled,
            content: &widgets.git.tools_audit_content,
            filter_button: &widgets.git.tools_audit_filter_button,
            filter_popover: &widgets.git.tools_audit_filter_popover,
            filter_store_box: &widgets.git.tools_audit_filter_store_box,
            filter_branch_box: &widgets.git.tools_audit_filter_branch_box,
        },
        root_list: &widgets.entries.list,
        root_search_entry: &widgets.entries.search_entry,
    })
}

fn window_page_state(widgets: &WindowWidgets, page: &adw::NavigationPage) -> WindowPageState {
    WindowPageState {
        window: widgets.shell.window.clone(),
        nav: widgets.shell.navigation.clone(),
        page: page.clone(),
        back: widgets.shell.back.clone(),
        add: widgets.entries.add_button.clone(),
        find: widgets.entries.find_button.clone(),
        primary_action: widgets.git.git_button.clone(),
        secondary_action: widgets.stores.store_button.clone(),
        save: widgets.entries.save_button.clone(),
        raw: widgets.entries.open_raw_button.clone(),
        title: widgets.shell.title.clone(),
    }
}

pub(super) fn preferences_action_state(
    widgets: &WindowWidgets,
    recipients_page: &StoreRecipientsPageState,
) -> PreferencesActionState {
    let generator_controls = widgets
        .preferences
        .map_password_generation_controls(PasswordGenerationControls::new);
    let controls = widgets
        .preferences
        .page_controls(generator_controls, &widgets.shell.overlay);

    PreferencesActionState {
        page_state: window_page_state(widgets, &widgets.preferences.page),
        controls,
        stores_list: widgets.preferences.stores.clone(),
        store_actions_list: widgets.preferences.store_actions.clone(),
        recipients_page: recipients_page.clone(),
    }
}

pub(super) fn build_git_action_state(
    widgets: &WindowWidgets,
    navigation: &WindowNavigationState,
    password_page: &PasswordPageState,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
) -> GitActionState {
    let restore_navigation = {
        let navigation = navigation.clone();
        let password_page = password_page.clone();
        let recipients_page = recipients_page.clone();
        let store_git_page = store_git_page.clone();
        Rc::new(move || {
            let _ = restore_window_for_current_page(
                &navigation,
                &password_page,
                &recipients_page,
                &store_git_page,
            );
        })
    };
    let refresh_window = widgets.shell.window.clone();
    let busy_window = widgets.shell.window.clone();
    GitActionState::new(
        &widgets.git,
        window_page_state(widgets, &widgets.git.git_busy_page),
        &widgets.shell.overlay,
        recipients_page,
        store_git_page,
        GitActionPorts {
            prompt_store_clone: Rc::new(|window, overlay, on_submit| {
                keycord_stores::ui::management::prompt_store_clone(
                    window,
                    overlay,
                    move |store, url| {
                        on_submit(store, url);
                    },
                );
            }),
            configured_stores: Rc::new(|| Preferences::new().stores()),
            set_configured_stores: Rc::new(|stores| {
                Preferences::new()
                    .set_stores(stores)
                    .map_err(|err| err.to_string())
            }),
            refresh_after_operation: Rc::new(move || {
                keycord_shell::actions::activate_widget_action(
                    &refresh_window,
                    "win.reload-password-list",
                );
            }),
            restore_navigation,
            set_application_busy: Rc::new(move |busy| {
                crate::composition::git_ui::set_application_busy(&busy_window, busy);
            }),
        },
    )
}

fn build_back_action_platform_state(git_action_state: &GitActionState) -> PlatformBackActionState {
    PlatformBackActionState {
        git_actions: git_action_state.clone(),
    }
}

pub(super) fn back_action_state(
    password_page: &PasswordPageState,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
    navigation: &WindowNavigationState,
    visibility: &PasswordListVisibilityState,
    git_action_state: &GitActionState,
) -> BackActionState {
    let platform = build_back_action_platform_state(git_action_state);

    BackActionState {
        password_page: password_page.clone(),
        recipients_page: recipients_page.clone(),
        store_git_page: store_git_page.clone(),
        navigation: navigation.clone(),
        visibility: visibility.clone(),
        platform,
    }
}

pub(super) fn context_undo_action_state(
    password_page: &PasswordPageState,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
    navigation: &WindowNavigationState,
    visibility: &PasswordListVisibilityState,
) -> ContextUndoActionState {
    let password_page_for_reload = password_page.clone();
    let visibility_for_reload = visibility.clone();
    let reload_password_list = Rc::new(move || {
        let actions = PasswordListActions::new(
            &password_page_for_reload.add,
            &password_page_for_reload.primary_action,
            &password_page_for_reload.secondary_action,
            &password_page_for_reload.find,
            &password_page_for_reload.save,
        );
        reload_password_list(
            &password_page_for_reload.list,
            &actions,
            &password_page_for_reload.overlay,
            &password_page_for_reload.nav,
            &visibility_for_reload,
        );
    });
    let navigation_for_restore = navigation.clone();
    let password_page_for_restore = password_page.clone();
    let recipients_page_for_restore = recipients_page.clone();
    let store_git_page_for_restore = store_git_page.clone();
    let restore_navigation = Rc::new(move || {
        let _ = restore_window_for_current_page(
            &navigation_for_restore,
            &password_page_for_restore,
            &recipients_page_for_restore,
            &store_git_page_for_restore,
        );
    });
    ContextUndoActionState::new(
        password_page,
        visibility,
        ContextUndoPorts::new(
            Arc::new(execute_undo_action),
            reload_password_list,
            restore_navigation,
        ),
    )
}
