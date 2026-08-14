//! GTK presentation owned by the preferences subject.

mod search;
mod settings;
mod widgets;

pub use search::{PreferencesPageSearchState, SearchablePreferencesGroup};
pub use settings::{
    connect_audit_history_recipient_row, connect_backend_row,
    connect_clear_empty_fields_before_save_autosave, connect_new_password_template_autosave,
    connect_pass_command_row, connect_password_generation_autosave,
    connect_password_list_sort_autosave, connect_private_key_sync_row,
    connect_username_fallback_autosave, initialize_backend_rows, sync_audit_history_recipient_row,
    sync_backend_rows, sync_clear_empty_fields_before_save_check,
    sync_password_generation_controls, sync_password_list_sort_checks, sync_private_key_sync_row,
    sync_username_fallback_checks, toast_preferences_save_error, PasswordGenerationControlsPort,
    PreferencesPageControls, PrivateKeySyncCallbacks,
};
pub use widgets::{
    configure_preferences_shortcuts, preferences_page_presentation, PreferencesWindowWidgets,
};
