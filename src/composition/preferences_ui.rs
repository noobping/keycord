use crate::composition::keys_sync::{
    preflight_host_to_app_private_key_sync, sync_private_keys_with_host,
};
use adw::gtk::{CheckButton, ListBox};
use adw::{ActionRow, ComboRow, EntryRow, ToastOverlay};
use keycord_entries::generation::PasswordGenerationControls;
use keycord_git::git_command_available;
use keycord_keys::PrivateKeySyncDirection;
use keycord_preferences::ui::{
    self as preferences_ui, preferences_page_presentation, PreferencesPageControls,
    PrivateKeySyncCallbacks,
};
use keycord_preferences::Preferences;
use keycord_shell::actions::{activate_widget_action, register_window_action};
use keycord_shell::navigation::{show_navigation_page, HasWindowChrome, WindowPageState};
use keycord_stores::ui::management::{rebuild_store_list, StoreRecipientsPageState};

pub fn initialize_backend_row(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    sync_row: &ActionRow,
    sync_check: &CheckButton,
    audit_row: &ActionRow,
    audit_check: &CheckButton,
    preferences: &Preferences,
) {
    preferences_ui::initialize_backend_rows(backend_row, pass_row, preferences);
    preferences_ui::sync_private_key_sync_row(
        sync_row,
        sync_check,
        preferences,
        keycord_keys::host_private_key_sync_available(),
    );
    preferences_ui::sync_audit_history_recipient_row(
        audit_row,
        audit_check,
        preferences,
        keycord_git::audit_available(),
        git_command_available(),
    );
}

pub fn connect_private_key_sync_row(state: &PreferencesActionState) {
    let window = state.page_state.window.clone();
    let window_for_callback = window.clone();
    let callbacks = PrivateKeySyncCallbacks::new(
        || {
            preflight_host_to_app_private_key_sync()
                .and_then(|_| sync_private_keys_with_host(PrivateKeySyncDirection::HostToApp))
                .map_err(|error| error.to_string())
        },
        move || {
            activate_widget_action(&window_for_callback, "win.reload-store-recipients-list");
            activate_widget_action(&window_for_callback, "win.reload-password-list");
        },
    );
    preferences_ui::connect_private_key_sync_row(
        &state.controls.sync_private_keys_row,
        &state.controls.sync_private_keys_check,
        &state.controls.overlay,
        &window,
        keycord_keys::host_private_key_sync_available(),
        callbacks,
    );
}

pub fn connect_audit_history_recipient_row(state: &PreferencesActionState) {
    preferences_ui::connect_audit_history_recipient_row(
        &state.controls.audit_use_commit_history_recipients_row,
        &state.controls.audit_use_commit_history_recipients_check,
        &state.controls.overlay,
        keycord_git::audit_available(),
        git_command_available(),
    );
}

fn refresh_preferences_page(state: &PreferencesActionState) {
    let settings = Preferences::new();
    state.controls.refresh_settings(
        &settings,
        keycord_keys::host_private_key_sync_available(),
        keycord_git::audit_available(),
        git_command_available(),
    );
    rebuild_store_list(
        &state.stores_list,
        &state.store_actions_list,
        &state.page_state.window,
        &state.controls.overlay,
        &state.recipients_page,
        None,
    );
    state.controls.search.sync();
}

fn show_preferences_page(state: &PreferencesActionState) {
    refresh_preferences_page(state);
    let chrome = state.page_state.window_chrome();
    show_navigation_page(
        &state.page_state.nav,
        &state.page_state.page,
        &chrome,
        &preferences_page_presentation(),
    );

    state
        .controls
        .focus_first_control(&[state.stores_list.clone(), state.store_actions_list.clone()]);
}

#[derive(Clone)]
pub struct PreferencesActionState {
    pub page_state: WindowPageState,
    pub controls: PreferencesPageControls<PasswordGenerationControls>,
    pub stores_list: ListBox,
    pub store_actions_list: ListBox,
    pub recipients_page: StoreRecipientsPageState,
}

pub fn connect_password_list_sort_autosave(
    filename_check: &CheckButton,
    hybrid_check: &CheckButton,
    store_path_check: &CheckButton,
    overlay: &ToastOverlay,
    window: &adw::ApplicationWindow,
) {
    let window = window.clone();
    preferences_ui::connect_password_list_sort_autosave(
        filename_check,
        hybrid_check,
        store_path_check,
        overlay,
        move || activate_widget_action(&window, "win.reload-password-list"),
    );
}

pub fn register_open_preferences_action(
    window: &adw::ApplicationWindow,
    state: &PreferencesActionState,
) {
    let state = state.clone();
    register_window_action(window, "open-preferences", move || {
        show_preferences_page(&state);
    });
}
