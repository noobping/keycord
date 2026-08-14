use super::widgets::WindowWidgets;
use crate::composition::entries_ui::{
    configure_password_list_store_filter, reload_password_list, setup_search_filter,
};
use crate::composition::host_access::append_optional_host_access_group_row;
use crate::composition::preferences_ui::{
    connect_audit_history_recipient_row, connect_password_list_sort_autosave,
    connect_private_key_sync_row, initialize_backend_row, register_open_preferences_action,
    PreferencesActionState,
};
use crate::window::controls::{
    register_back_action, register_context_reload_action, register_context_save_action,
    register_context_undo_action, register_go_home_action, register_toggle_find_action,
    BackActionState, ToggleFindCallback,
};
use crate::window::navigation::WindowNavigationState;
use crate::window::tool_hub::{register_open_tools_action, ToolHubState};
use adw::prelude::*;
use keycord_docs::{register_open_docs_action, DocumentationPageState};
use keycord_entries::ui::actions::{
    configure_password_save_button, connect_new_password_submit, connect_password_copy_buttons,
    connect_password_list_activation, register_password_page_actions,
};
use keycord_entries::ui::list::{
    connect_password_list_search_visibility, connect_selected_pass_file_shortcuts,
    register_password_list_window_actions, PasswordListActions, PasswordListVisibilityState,
};
use keycord_entries::ui::new_item::{register_open_new_password_action, NewPasswordDialogState};
use keycord_entries::ui::page::PasswordPageState;
use keycord_entries::ui::undo::{context_undo_callback, ContextUndoActionState};
use keycord_git::ui::{connect_store_git_controls, StoreGitPageState};
use keycord_git::ui::{
    register_open_git_action, register_synchronize_action, set_git_action_availability,
    GitActionState,
};
use keycord_preferences::ui::{
    connect_backend_row, connect_clear_empty_fields_before_save_autosave,
    connect_new_password_template_autosave, connect_pass_command_row,
    connect_password_generation_autosave, connect_username_fallback_autosave,
};
use keycord_preferences::Preferences;
use keycord_runtime::capabilities::{
    has_host_permission, supports_host_command_features, supports_logging_features,
};
use keycord_runtime::log_info;
use keycord_shell::actions::activate_widget_action;
use keycord_shell::deferred::DeferredState;
use keycord_stores::ui::management::{
    connect_store_recipients_controls, initialize_store_import_page, rebuild_store_actions_list,
    register_open_store_picker_action, register_open_store_recipients_shortcut_actions,
    register_store_recipients_reload_action, register_store_recipients_save_action,
    StoreImportPageState, StoreRecipientsPageState,
};
use std::rc::Rc;

pub(super) fn assemble_password_list_page(
    widgets: &WindowWidgets,
    visibility: &PasswordListVisibilityState,
) {
    let primary_menu_button = widgets.shell.primary_menu.clone().upcast();
    setup_search_filter(
        &widgets.entries.list,
        &widgets.entries.search_entry,
        &primary_menu_button,
        &widgets.entries.password_list_stack,
        &widgets.entries.password_list_status,
        &widgets.entries.password_list_spinner,
        &widgets.entries.password_list_scrolled,
    );
    configure_password_list_store_filter(
        &widgets.entries.password_list_filter_button,
        &widgets.entries.password_list_filter_popover,
        &widgets.entries.password_list_filter_store_box,
        &widgets.entries.list,
        &widgets.shell.navigation,
        &widgets.shell.overlay,
    );
    connect_selected_pass_file_shortcuts(&widgets.entries.list, &widgets.shell.overlay);

    let list_actions = PasswordListActions::new(
        &widgets.entries.add_button,
        &widgets.git.git_button,
        &widgets.stores.store_button,
        &widgets.entries.find_button,
        &widgets.entries.save_button,
    );
    reload_password_list(
        &widgets.entries.list,
        &list_actions,
        &widgets.shell.overlay,
        &widgets.shell.navigation,
        visibility,
    );
}

pub(super) fn assemble_password_page(
    widgets: &WindowWidgets,
    password_page_state: &PasswordPageState,
    preferences_action_state: &PreferencesActionState,
    new_password_dialog_state: &NewPasswordDialogState,
) {
    configure_password_save_button(&widgets.entries.save_button);
    configure_password_save_button(&widgets.entries.editor_save_button);

    connect_password_list_activation(
        &widgets.entries.list,
        &widgets.entries.search_entry,
        &widgets.shell.overlay,
        password_page_state,
    );
    connect_password_copy_buttons(
        &widgets.shell.overlay,
        (
            &widgets.entries.password_entry,
            &widgets.entries.copy_password_button,
            &widgets.entries.qr_password_button,
        ),
        (
            &widgets.entries.username_entry,
            &widgets.entries.copy_username_button,
            &widgets.entries.qr_username_button,
        ),
        (
            &widgets.entries.otp_entry,
            &widgets.entries.copy_otp_button,
            &widgets.entries.qr_otp_button,
        ),
    );
    connect_new_password_submit(password_page_state, new_password_dialog_state);
    connect_password_generation_autosave(
        &password_page_state.generator_controls,
        std::slice::from_ref(&preferences_action_state.controls.generator_controls),
        &widgets.shell.overlay,
    );

    password_page_state.connect_generator_settings_control();

    register_password_page_actions(&widgets.shell.window, password_page_state);
    register_open_new_password_action(&widgets.shell.window, new_password_dialog_state);
}

pub(super) fn assemble_preferences_page(
    widgets: &WindowWidgets,
    preferences: &Preferences,
    password_page_state: &PasswordPageState,
    preferences_action_state: &PreferencesActionState,
    tool_hub_state: &DeferredState<ToolHubState>,
) {
    preferences_action_state.controls.search.connect_handlers();
    initialize_backend_preferences(widgets, preferences);

    connect_new_password_template_autosave(
        &widgets.preferences.template_view,
        &widgets.shell.overlay,
    );
    connect_clear_empty_fields_before_save_autosave(
        &preferences_action_state
            .controls
            .clear_empty_fields_before_save_row,
        &preferences_action_state
            .controls
            .clear_empty_fields_before_save_check,
        &widgets.shell.overlay,
    );
    connect_username_fallback_autosave(
        &widgets.preferences.username_folder_check,
        &widgets.preferences.username_filename_check,
        &widgets.shell.overlay,
    );
    connect_password_list_sort_autosave(
        &widgets.preferences.password_list_sort_filename_check,
        &widgets.preferences.password_list_sort_hybrid_check,
        &widgets.preferences.password_list_sort_store_path_check,
        &widgets.shell.overlay,
        &widgets.shell.window,
    );
    connect_password_generation_autosave(
        &preferences_action_state.controls.generator_controls,
        std::slice::from_ref(&password_page_state.generator_controls),
        &widgets.shell.overlay,
    );
    connect_backend_preferences(
        widgets,
        preferences,
        preferences_action_state,
        tool_hub_state,
    );

    register_open_preferences_action(&widgets.shell.window, preferences_action_state);
}

pub(super) fn assemble_store_import_page(
    widgets: &WindowWidgets,
    store_recipients_page: &StoreRecipientsPageState,
) {
    let state = StoreImportPageState::new(
        &widgets.stores,
        &widgets.shell.window,
        &widgets.shell.navigation,
        &store_recipients_page.ports,
        &widgets.shell.overlay,
    );
    initialize_store_import_page(&state);
}

pub(super) fn assemble_store_recipients_page(
    widgets: &WindowWidgets,
    store_recipients_page_state: &StoreRecipientsPageState,
) {
    store_recipients_page_state.search.connect_handlers();
    connect_store_recipients_controls(store_recipients_page_state);
    register_store_recipients_save_action(
        &widgets.shell.window,
        &widgets.shell.overlay,
        &widgets.preferences.stores,
        store_recipients_page_state,
    );
    register_store_recipients_reload_action(&widgets.shell.window, store_recipients_page_state);
    register_open_store_picker_action(
        &widgets.shell.window,
        &widgets.preferences.stores,
        &widgets.shell.overlay,
        store_recipients_page_state,
    );
    register_open_store_recipients_shortcut_actions(
        &widgets.shell.window,
        store_recipients_page_state,
    );
}

pub(super) fn assemble_git_page(
    widgets: &WindowWidgets,
    store_git_page: &StoreGitPageState,
    git_action_state: &GitActionState,
) {
    store_git_page.search.connect_handlers();
    let git_supported = supports_host_command_features();
    connect_store_git_controls(store_git_page);
    if git_supported {
        register_open_git_action(git_action_state);
        register_synchronize_action(git_action_state);
    }

    let git_available = git_supported && has_host_permission();
    set_git_action_availability(&widgets.shell.window, git_available);
    log_info(format!(
        "Window Git actions: open-git, git-clone, and synchronize are {}.",
        if git_available { "enabled" } else { "disabled" }
    ));
}

pub(super) fn assemble_log_page(widgets: &WindowWidgets, navigation_state: &WindowNavigationState) {
    let logging_supported = supports_logging_features();
    widgets.shell.set_logging_available(logging_supported);
    widgets.git.set_busy_logs_available(logging_supported);

    if !logging_supported {
        return;
    }

    #[cfg(feature = "logging")]
    {
        let navigation_state = navigation_state.clone();
        keycord_shell::logs::register_open_log_action(&widgets.shell.window, move || {
            crate::window::navigation::show_log_page(&navigation_state);
        });
        keycord_shell::logs::start_log_poller(&widgets.shell.log_view);
    }

    #[cfg(not(feature = "logging"))]
    let _ = navigation_state;
}

pub(super) fn assemble_docs_page(
    widgets: &WindowWidgets,
    docs_page_state: &DeferredState<DocumentationPageState>,
) {
    let docs_page_state = docs_page_state.clone();
    register_open_docs_action(&widgets.shell.window, move || {
        docs_page_state.with(DocumentationPageState::open);
    });
}

pub(super) fn assemble_tools_page(
    widgets: &WindowWidgets,
    tool_hub_state: &DeferredState<ToolHubState>,
) {
    let tool_hub_state = tool_hub_state.clone();
    register_open_tools_action(&widgets.shell.window, move || {
        tool_hub_state.with(ToolHubState::open);
    });
}

pub(super) fn register_window_navigation_actions(
    widgets: &WindowWidgets,
    navigation_state: &WindowNavigationState,
    tool_hub_state: &DeferredState<ToolHubState>,
    store_recipients_page_state: &StoreRecipientsPageState,
    list_visibility: &PasswordListVisibilityState,
    back_action_state: &BackActionState,
    context_undo_state: &ContextUndoActionState,
) {
    register_context_save_action(
        &widgets.shell.window,
        navigation_state,
        store_recipients_page_state,
    );
    register_context_reload_action(
        &widgets.shell.window,
        navigation_state,
        store_recipients_page_state,
    );
    register_context_undo_action(
        &widgets.shell.window,
        context_undo_callback(context_undo_state),
    );

    connect_password_list_search_visibility(
        &widgets.entries.find_button,
        &widgets.entries.search_entry,
        &widgets.entries.list,
    );
    let navigation = navigation_state.nav.clone();
    let find_button = navigation_state.find.clone();
    let preferences = widgets.preferences.clone();
    let stores = widgets.stores.clone();
    let git = widgets.git.clone();
    let docs = widgets.docs.clone();
    let tool_hub = widgets.tool_hub.clone();
    let entries = widgets.entries.clone();
    let on_audit_search_changed: Rc<dyn Fn()> = Rc::new({
        let tool_hub_state = tool_hub_state.clone();
        move || {
            let _ = tool_hub_state.with_initialized(|state| state.render_audit_page());
        }
    });
    let toggle_find_callbacks: Vec<ToggleFindCallback> = vec![
        Rc::new({
            let navigation = navigation.clone();
            let find_button = find_button.clone();
            move || preferences.toggle_find_for_visible_page(&navigation, &find_button)
        }),
        Rc::new({
            let navigation = navigation.clone();
            let find_button = find_button.clone();
            move || stores.toggle_find_for_visible_page(&navigation, &find_button)
        }),
        Rc::new({
            let navigation = navigation.clone();
            let find_button = find_button.clone();
            move || {
                git.toggle_find_for_visible_page(
                    &navigation,
                    &find_button,
                    on_audit_search_changed.as_ref(),
                )
            }
        }),
        Rc::new({
            let navigation = navigation.clone();
            let find_button = find_button.clone();
            move || docs.toggle_find_for_visible_page(&navigation, &find_button)
        }),
        Rc::new({
            let navigation = navigation.clone();
            let find_button = find_button.clone();
            move || tool_hub.toggle_find_for_visible_page(&navigation, &find_button)
        }),
        Rc::new(move || entries.toggle_find_for_visible_page(&navigation)),
    ];
    register_toggle_find_action(&widgets.shell.window, toggle_find_callbacks);
    let list_actions = PasswordListActions::new(
        &navigation_state.add,
        &navigation_state.primary_action,
        &navigation_state.secondary_action,
        &navigation_state.find,
        &navigation_state.save,
    );
    register_password_list_window_actions(
        &widgets.shell.window,
        &navigation_state.nav,
        list_visibility,
        Rc::new({
            let list = widgets.entries.list.clone();
            let actions = list_actions;
            let overlay = widgets.shell.overlay.clone();
            let navigation = navigation_state.nav.clone();
            let visibility = list_visibility.clone();
            move || {
                reload_password_list(&list, &actions, &overlay, &navigation, &visibility);
            }
        }),
    );
    register_go_home_action(&widgets.shell.window, back_action_state);
    register_back_action(&widgets.shell.window, back_action_state);
}

fn initialize_backend_preferences(widgets: &WindowWidgets, preferences: &Preferences) {
    let host_features_supported = supports_host_command_features();
    widgets
        .preferences
        .set_host_features_available(host_features_supported);
    initialize_backend_row(
        &widgets.preferences.backend_row,
        &widgets.preferences.pass_command_row,
        &widgets.preferences.sync_private_keys_row,
        &widgets.preferences.sync_private_keys_check,
        &widgets.preferences.audit_history_recipients_row,
        &widgets.preferences.audit_history_recipients_check,
        preferences,
    );
    if !host_features_supported {
        return;
    }

    append_optional_host_access_group_row(
        &widgets.preferences.host_access_group,
        &widgets.shell.overlay,
    );
}

fn connect_backend_preferences(
    widgets: &WindowWidgets,
    preferences: &Preferences,
    preferences_action_state: &PreferencesActionState,
    tool_hub_state: &DeferredState<ToolHubState>,
) {
    connect_pass_command_row(
        &widgets.preferences.pass_command_row,
        &widgets.shell.overlay,
        preferences,
    );
    connect_private_key_sync_row(preferences_action_state);
    connect_audit_history_recipient_row(preferences_action_state);
    connect_backend_row(
        &widgets.preferences.backend_row,
        &widgets.preferences.pass_command_row,
        &widgets.shell.overlay,
        preferences,
        {
            let preferences_action_state = preferences_action_state.clone();
            let tool_hub_state = tool_hub_state.clone();
            let window = widgets.shell.window.clone();
            move || {
                let _ = tool_hub_state.with_initialized(ToolHubState::refresh_select_page);
                rebuild_store_actions_list(
                    &preferences_action_state.store_actions_list,
                    &preferences_action_state.stores_list,
                    &preferences_action_state.page_state.window,
                    &preferences_action_state.controls.overlay,
                    &preferences_action_state.recipients_page,
                    None,
                );
                preferences_action_state.controls.search.sync();
                activate_widget_action(&window, "win.reload-store-recipients-list");
            }
        },
    );
}
