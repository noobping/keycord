mod actions;
mod assemble;
mod chrome;
mod deferred;
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
use self::deferred::DeferredState;
use self::state::{
    back_action_state, build_git_action_state, context_undo_action_state, docs_page_state,
    list_visibility_action_state, new_password_dialog_state, password_page_state,
    preferences_action_state, store_git_page_state, store_recipients_page_state, tools_page_state,
    window_navigation_state,
};
use self::widgets::WindowWidgets;
use crate::logging::log_error;
use crate::password::model::OpenPassFile;
use crate::password::otp::PasswordOtpState;
#[cfg(feature = "passkey")]
use crate::password::page::password_page_would_discard_work;
use crate::password::page::{open_password_entry_page, password_page_has_unsaved_changes};
#[cfg(feature = "passkey")]
use crate::password::passkey::{encode_passkey_envelope, PasskeyCredential};
use crate::preferences::Preferences;
use crate::private_key::sync::{sync_private_keys_with_host, PrivateKeySyncDirection};
use crate::support::actions::activate_widget_action;
use crate::support::object_data::{cloned_data, set_cloned_data};
use crate::support::runtime::log_runtime_capabilities_once;
use crate::window::controls::{
    apply_startup_query, configure_window_shortcuts, ListVisibilityState,
};
use crate::window::session::initialize_window_session;
use adw::gtk::{Builder, ListBox, SearchEntry};
use adw::{prelude::*, Application, ApplicationWindow};
use std::rc::Rc;

const UI_SRC: &str = include_str!(concat!(env!("OUT_DIR"), "/window.ui"));
const MAIN_WINDOW_COMMAND_STATE_KEY: &str = "main-window-command-state";

#[derive(Clone)]
struct MainWindowCommandState {
    list: ListBox,
    search_entry: SearchEntry,
    password_page: crate::password::page::PasswordPageState,
}

pub fn create_main_window(
    app: &Application,
    startup_query: Option<String>,
    initial_pass_file: Option<OpenPassFile>,
) -> Result<ApplicationWindow, String> {
    let builder = Builder::from_string(UI_SRC);
    let widgets = WindowWidgets::load(&builder)?;
    widgets.window.set_application(Some(app));
    initialize_window_session(&widgets.window);
    log_runtime_capabilities_once();

    let preferences = Preferences::new();
    if preferences.sync_private_keys_with_host() {
        if let Err(err) = sync_private_keys_with_host(PrivateKeySyncDirection::HostToApp) {
            log_error(format!("Failed to sync private keys during startup: {err}"));
            let _ = preferences.set_sync_private_keys_with_host(false);
        }
    }

    initialize_window_chrome(&widgets, &preferences);

    let new_password_dialog_state = new_password_dialog_state(&widgets);
    let password_otp_state = PasswordOtpState::new(&widgets.otp_entry, &widgets.toast_overlay);
    let password_page_state = password_page_state(&widgets, &password_otp_state);
    set_cloned_data(
        &widgets.window,
        MAIN_WINDOW_COMMAND_STATE_KEY,
        MainWindowCommandState {
            list: widgets.list.clone(),
            search_entry: widgets.search_entry.clone(),
            password_page: password_page_state.clone(),
        },
    );
    let list_visibility = ListVisibilityState::new(false, false);
    let store_git_page_state = store_git_page_state(&widgets);
    let store_recipients_page_state = store_recipients_page_state(&widgets, &store_git_page_state);
    let window_navigation_state = window_navigation_state(&widgets);
    let docs_page_state = DeferredState::new({
        let widgets = widgets.clone();
        let window_navigation_state = window_navigation_state.clone();
        move || docs_page_state(&widgets, &window_navigation_state)
    });
    let tools_page_state = DeferredState::new({
        let widgets = widgets.clone();
        let window_navigation_state = window_navigation_state.clone();
        let password_page_state = password_page_state.clone();
        move || tools_page_state(&widgets, &window_navigation_state, &password_page_state)
    });
    let preferences_action_state = preferences_action_state(&widgets, &store_recipients_page_state);
    let git_action_state = build_git_action_state(
        &widgets,
        &window_navigation_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &list_visibility,
    );
    let back_action_state = back_action_state(
        &password_page_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &window_navigation_state,
        &list_visibility,
        &git_action_state,
    );
    let list_visibility_action_state =
        list_visibility_action_state(&widgets, &window_navigation_state, &list_visibility);
    let context_undo_state = context_undo_action_state(
        &password_page_state,
        &store_recipients_page_state,
        &store_git_page_state,
        &window_navigation_state,
        &list_visibility,
    );

    assemble_password_list_page(&widgets);
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
        &tools_page_state,
    );
    assemble_store_import_page(&widgets, &window_navigation_state);
    assemble_store_recipients_page(&widgets, &store_recipients_page_state);
    assemble_git_page(&widgets, &store_git_page_state, &git_action_state);
    assemble_log_page(&widgets, &window_navigation_state);
    assemble_docs_page(&widgets, &docs_page_state);
    assemble_tools_page(&widgets, &tools_page_state);
    register_window_navigation_actions(
        &widgets,
        &window_navigation_state,
        &tools_page_state,
        &store_recipients_page_state,
        &list_visibility_action_state,
        &back_action_state,
        &context_undo_state,
    );
    connect_window_keyboard_navigation(&widgets, &window_navigation_state);

    crate::updater::register_window(
        app,
        &widgets.window,
        &widgets.toast_overlay,
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
    apply_startup_query(startup_query, &widgets.search_entry, &widgets.list);
    if let Some(initial_pass_file) = initial_pass_file {
        open_password_entry_page(&password_page_state, initial_pass_file, true);
    } else {
        schedule_initial_focus(&widgets, &window_navigation_state);
    }

    Ok(widgets.window)
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
    apply_startup_query(Some(query), &state.search_entry, &state.list);
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
    let envelope = encode_passkey_envelope(credential)?;
    let contents = format!("\npasskey: {envelope}");
    let label = passkey_entry_label(credential);
    crate::password::page::begin_new_password_entry_with_contents(
        &state.password_page,
        &label,
        None,
        &contents,
    )
    .map_err(str::to_string)
}

#[cfg(feature = "passkey")]
fn passkey_entry_label(credential: &PasskeyCredential) -> String {
    let username = safe_passkey_label_component(&credential.username);
    let credential_id = credential
        .credential_id
        .chars()
        .take(12)
        .collect::<String>();
    format!(
        "passkeys/{}/{}-{}",
        credential.rp_id, username, credential_id
    )
}

#[cfg(feature = "passkey")]
fn safe_passkey_label_component(value: &str) -> String {
    let mut output = String::new();
    let mut previous_separator = false;
    for character in value.chars().take(64) {
        if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
            output.push(character);
            previous_separator = false;
        } else if !previous_separator {
            output.push('-');
            previous_separator = true;
        }
    }
    let output = output.trim_matches(['.', '_', '-']);
    if output.is_empty() || matches!(output, "." | "..") {
        "user".to_string()
    } else {
        output.to_string()
    }
}

#[cfg(all(test, feature = "passkey"))]
mod passkey_import_tests {
    use super::{passkey_entry_label, safe_passkey_label_component};
    use crate::password::passkey::{PasskeyCredential, PasskeyRegistrationState};

    #[test]
    fn imported_passkey_labels_stay_inside_the_pass_store() {
        let credential = PasskeyCredential {
            credential_id: "credential_id".to_string(),
            rp_id: "example.com".to_string(),
            username: "../../Alice / Admin".to_string(),
            user_display_name: "Alice".to_string(),
            user_handle: "handle".to_string(),
            key: "private".to_string(),
            fido2_extensions: None,
            registration_state: PasskeyRegistrationState::Imported,
        };

        assert_eq!(
            passkey_entry_label(&credential),
            "passkeys/example.com/Alice-Admin-credential_i"
        );
        assert!(!passkey_entry_label(&credential).contains(".."));
        assert_eq!(safe_passkey_label_component("..."), "user");
    }
}
