mod assemble;
mod chrome;
mod state;
pub(super) mod widgets;

use self::assemble::{
    assemble_docs_page, assemble_git_page, assemble_log_page, assemble_password_list_page,
    assemble_password_page, assemble_preferences_page, assemble_store_import_page,
    assemble_store_recipients_page, assemble_tools_page, register_window_navigation_actions,
};
use self::chrome::{
    connect_window_keyboard_navigation, initialize_window_chrome, schedule_initial_focus,
};
use self::state::{
    back_action_state, build_git_action_state, context_undo_action_state, docs_page_state,
    new_password_dialog_state, password_page_state, preferences_action_state, store_git_page_state,
    store_recipients_page_state, tool_hub_state, window_navigation_state,
};
use self::widgets::WindowWidgets;
use crate::composition::keys_sync::sync_private_keys_with_host;
use crate::window::controls::configure_window_shortcuts;
use adw::gtk::{Builder, ListBox, SearchEntry};
use adw::{prelude::*, Application, ApplicationWindow};
use keycord_entries::model::OpenPassFile;
use keycord_entries::otp::PasswordOtpState;
use keycord_entries::ui::list::{apply_password_list_startup_query, PasswordListVisibilityState};
#[cfg(feature = "passkey")]
use keycord_entries::ui::page::password_page_would_discard_work;
use keycord_entries::ui::page::{open_password_entry_page, password_page_has_unsaved_changes};
use keycord_entries::ui::session::initialize_window_session;
use keycord_keys::PrivateKeySyncDirection;
#[cfg(feature = "passkey")]
use keycord_passkey::{build_passkey_storage_entry, PasskeyCredential};
use keycord_preferences::Preferences;
use keycord_runtime::capabilities::log_runtime_capabilities_once;
use keycord_runtime::log_error;
use keycord_shell::actions::activate_widget_action;
use keycord_shell::deferred::DeferredState;
use keycord_shell::object_data::{cloned_data, set_cloned_data};
use std::rc::Rc;

const UI_SRC: &str = include_str!(concat!(env!("OUT_DIR"), "/window.ui"));
const MAIN_WINDOW_COMMAND_STATE_KEY: &str = "main-window-command-state";

#[derive(Clone)]
struct MainWindowCommandState {
    list: ListBox,
    search_entry: SearchEntry,
    password_page: keycord_entries::ui::page::PasswordPageState,
}

pub fn create_main_window(
    app: &Application,
    startup_query: Option<String>,
    initial_pass_file: Option<OpenPassFile>,
) -> Result<ApplicationWindow, String> {
    let builder = Builder::from_string(UI_SRC);
    let widgets = WindowWidgets::load(&builder)?;
    widgets.shell.window.set_application(Some(app));
    initialize_window_session(&widgets.shell.window);
    log_runtime_capabilities_once();

    let preferences = Preferences::new();
    if crate::composition::keys_sync::private_key_sync_enabled() {
        if let Err(err) = sync_private_keys_with_host(PrivateKeySyncDirection::HostToApp) {
            log_error(format!("Failed to sync private keys during startup: {err}"));
            let _ = preferences.set_sync_private_keys_with_host(false);
        }
    }

    initialize_window_chrome(&widgets, &preferences);

    let new_password_dialog_state = new_password_dialog_state(&widgets);
    let password_otp_state =
        PasswordOtpState::new(&widgets.entries.otp_entry, &widgets.shell.overlay);
    let password_page_state = password_page_state(&widgets, &password_otp_state);
    set_cloned_data(
        &widgets.shell.window,
        MAIN_WINDOW_COMMAND_STATE_KEY,
        MainWindowCommandState {
            list: widgets.entries.list.clone(),
            search_entry: widgets.entries.search_entry.clone(),
            password_page: password_page_state.clone(),
        },
    );
    let list_visibility = PasswordListVisibilityState::new(false, false);
    let store_git_page_state = store_git_page_state(&widgets);
    let store_recipients_page_state = store_recipients_page_state(&widgets, &store_git_page_state);
    let window_navigation_state = window_navigation_state(&widgets);
    let docs_page_state = DeferredState::new({
        let widgets = widgets.clone();
        let window_navigation_state = window_navigation_state.clone();
        move || docs_page_state(&widgets, &window_navigation_state)
    });
    let tool_hub_state = DeferredState::new({
        let widgets = widgets.clone();
        let window_navigation_state = window_navigation_state.clone();
        let password_page_state = password_page_state.clone();
        let store_ports = store_recipients_page_state.ports.clone();
        move || {
            tool_hub_state(
                &widgets,
                &window_navigation_state,
                &password_page_state,
                &store_ports,
            )
        }
    });
    let preferences_action_state = preferences_action_state(&widgets, &store_recipients_page_state);
    let git_action_state = build_git_action_state(
        &widgets,
        &window_navigation_state,
        &password_page_state,
        &store_recipients_page_state,
        &store_git_page_state,
    );
    let back_action_state = back_action_state(
        &password_page_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &window_navigation_state,
        &list_visibility,
        &git_action_state,
    );
    let context_undo_state = context_undo_action_state(
        &password_page_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &window_navigation_state,
        &list_visibility,
    );

    assemble_password_list_page(&widgets, &list_visibility);
    assemble_password_page(
        &widgets,
        &password_page_state,
        &preferences_action_state,
        &new_password_dialog_state,
    );
    assemble_preferences_page(
        &widgets,
        &preferences,
        &password_page_state,
        &preferences_action_state,
        &tool_hub_state,
    );
    assemble_store_import_page(&widgets, &store_recipients_page_state);
    assemble_store_recipients_page(&widgets, &store_recipients_page_state);
    assemble_git_page(&widgets, &store_git_page_state, &git_action_state);
    assemble_log_page(&widgets, &window_navigation_state);
    assemble_docs_page(&widgets, &docs_page_state);
    assemble_tools_page(&widgets, &tool_hub_state);
    register_window_navigation_actions(
        &widgets,
        &window_navigation_state,
        &tool_hub_state,
        &store_recipients_page_state,
        &list_visibility,
        &back_action_state,
        &context_undo_state,
    );
    connect_window_keyboard_navigation(&widgets, &window_navigation_state);

    crate::updater::register_window(
        app,
        &widgets.shell.window,
        &widgets.shell.overlay,
        Rc::new({
            let password_page = password_page_state.clone();
            let recipients_page = store_recipients_page_state.clone();
            move || {
                password_page_has_unsaved_changes(&password_page)
                    || recipients_page.recipients_are_dirty()
            }
        }),
    );

    configure_window_shortcuts(app);
    apply_password_list_startup_query(
        startup_query,
        &widgets.entries.search_entry,
        &widgets.entries.list,
    );
    if let Some(initial_pass_file) = initial_pass_file {
        open_password_entry_page(&password_page_state, initial_pass_file, true);
    } else {
        schedule_initial_focus(&widgets, &window_navigation_state);
    }

    Ok(widgets.shell.window)
}

pub fn dispatch_main_window_command(
    window: &ApplicationWindow,
    startup_query: Option<String>,
    initial_pass_file: Option<OpenPassFile>,
) {
    let Some(state) =
        cloned_data::<_, MainWindowCommandState>(window, MAIN_WINDOW_COMMAND_STATE_KEY)
    else {
        return;
    };

    if let Some(initial_pass_file) = initial_pass_file {
        open_password_entry_page(&state.password_page, initial_pass_file, true);
        return;
    }

    let Some(query) = startup_query else {
        return;
    };
    if query.is_empty() {
        return;
    }

    activate_widget_action(window, "win.go-home");
    apply_password_list_startup_query(Some(query), &state.search_entry, &state.list);
}

#[cfg(feature = "passkey")]
pub fn begin_passkey_import(
    window: &ApplicationWindow,
    credential: &PasskeyCredential,
) -> Result<(), String> {
    let state = cloned_data::<_, MainWindowCommandState>(window, MAIN_WINDOW_COMMAND_STATE_KEY)
        .ok_or_else(|| "The password editor is not available.".to_string())?;
    if password_page_would_discard_work(&state.password_page) {
        return Err("Save or discard your current changes before importing a passkey.".to_string());
    }
    let entry = build_passkey_storage_entry(credential)?;
    keycord_entries::ui::page::begin_new_password_entry_with_contents(
        &state.password_page,
        &entry.label,
        None,
        &entry.contents,
    )
    .map_err(str::to_string)
}
