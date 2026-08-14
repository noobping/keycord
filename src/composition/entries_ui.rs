//! Connects Entries UI ports to selected application backends and peer subjects.

use adw::gtk::{Box as GtkBox, ListBox, MenuButton, Popover, SearchEntry, Widget};
use adw::prelude::*;
use adw::{ApplicationWindow, NavigationView, ToastOverlay};
use keycord_entries::clipboard::EntryClipboardPorts;
use keycord_entries::model::{CollectItemsOptions, PassEntry};
use keycord_entries::tools::EntryRequest;
use keycord_entries::ui::list::{
    EntryListBackendPorts, EntryListPreferencesPorts, EntryListUiPorts, PasswordListActions,
    PasswordListSearchWidgets, PasswordListVisibilityState,
};
use keycord_entries::ui::page::{
    EntryPageBackendPorts, EntryPageKeyPorts, EntryPagePreferencesPorts, EntryPageUiPorts,
};
use keycord_entries::ui::tools::{
    EntryToolBackendPorts, EntryToolKeyPorts, EntryToolKeySummary, EntryToolPreferencesPorts,
    EntryToolStorePorts, EntryToolUiPorts,
};
use keycord_entries::undo::{EntryOperationPort, EntryUndoBackend, UndoAction, UndoError};
use keycord_entries::{PasswordEntryError, PasswordEntryWriteError};
use keycord_preferences::{password_store_command_log_options, BackendKind, Preferences};
use keycord_runtime::{run_command_status, CommandLogOptions};
use std::rc::Rc;
use std::sync::Arc;

struct RootEntryOperationPort;

static ROOT_ENTRY_OPERATION_PORT: RootEntryOperationPort = RootEntryOperationPort;

impl EntryOperationPort for RootEntryOperationPort {
    fn read_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, PasswordEntryError> {
        crate::composition::backend::read_password_entry(store_root, label)
    }

    fn save_password_entry(
        &self,
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError> {
        crate::composition::backend::save_password_entry(store_root, label, contents, overwrite)
    }

    fn rename_password_entry(
        &self,
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), PasswordEntryWriteError> {
        crate::composition::backend::rename_password_entry(store_root, old_label, new_label)
    }

    fn delete_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<(), PasswordEntryWriteError> {
        crate::composition::backend::delete_password_entry(store_root, label)
    }
}

fn undo_backend() -> EntryUndoBackend<'static> {
    EntryUndoBackend::new(&ROOT_ENTRY_OPERATION_PORT)
}

pub fn collect_all_password_items_with_options(options: CollectItemsOptions) -> Vec<PassEntry> {
    let preferences = Preferences::new();
    keycord_entries::model::collect_password_items_with_options(
        &preferences.paths(),
        preferences.password_list_sort_mode(),
        options,
        keycord_stores::recipients::store_is_supported_in_current_build,
    )
}

pub fn delete_entry_with_optional_undo(entry: &PassEntry) -> Result<Option<UndoAction>, UndoError> {
    undo_backend().delete_entry_with_optional_undo(entry)
}

pub fn move_entry_to_store(entry: &PassEntry, target_store: &str) -> Result<PassEntry, UndoError> {
    undo_backend().move_entry_to_store(entry, target_store)
}

pub fn execute_undo_action(action: &UndoAction) -> Result<(), UndoError> {
    undo_backend().execute_undo_action(action)
}

pub fn entry_clipboard_ports() -> EntryClipboardPorts {
    EntryClipboardPorts::new(
        || Preferences::new().uses_integrated_backend(),
        Arc::new(|store, label| crate::composition::backend::read_password_line(&store, &label)),
        Arc::new(|store, label| {
            keycord_stores::integrated_recipients::preferred_ripasso_private_key_fingerprint_for_entry(
                &store,
                &label,
            )
        }),
        Arc::new(|item| {
            let preferences = Preferences::new();
            let mut command = preferences.command();
            command
                .env("PASSWORD_STORE_DIR", &item.store_path)
                .arg("-c")
                .arg(item.label());
            run_command_status(
                &mut command,
                "Copy password to clipboard",
                password_store_command_log_options(CommandLogOptions::SENSITIVE),
            )
            .map(|_| ())
            .map_err(|err| err.to_string())
        }),
        Rc::new(crate::composition::keys_unlock::prompt_private_key_unlock_for_action),
    )
}

pub fn entry_list_ports() -> EntryListUiPorts {
    EntryListUiPorts {
        preferences: EntryListPreferencesPorts {
            stores: Rc::new(|| Preferences::new().stores()),
            store_roots: Rc::new(|| Preferences::new().store_roots()),
            prune_missing_stores: Rc::new(|| {
                Preferences::new()
                    .prune_missing_stores()
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            }),
            included_store_roots: Rc::new(|| Preferences::new().filter_included_store_roots()),
            set_included_store_roots: Rc::new(|roots| {
                Preferences::new()
                    .set_filter_included_store_roots(roots)
                    .map_err(|err| err.to_string())
            }),
            sort_mode: Rc::new(|| Preferences::new().password_list_sort_mode()),
        },
        backend: EntryListBackendPorts {
            collect_items: Arc::new(collect_all_password_items_with_options),
            is_readable: Arc::new(|store, label| {
                crate::composition::backend::password_entry_is_readable(&store, &label)
            }),
            read_entry: Arc::new(|store, label| {
                crate::composition::backend::read_password_entry(&store, &label)
                    .map_err(|err| err.to_string())
            }),
            rename_entry: Arc::new(|store, old_label, new_label| {
                crate::composition::backend::rename_password_entry(&store, &old_label, &new_label)
            }),
            delete_entry_with_undo: Arc::new(|entry| delete_entry_with_optional_undo(&entry)),
            move_entry_to_store: Arc::new(|entry, target_store| {
                move_entry_to_store(&entry, &target_store)
            }),
        },
        clipboard: entry_clipboard_ports(),
        open_entry_window: Rc::new(|app, entry| {
            crate::window::create_main_window(
                app,
                None,
                Some(keycord_entries::model::OpenPassFile::new(entry)),
            )
            .map_err(|err| err.to_string())
        }),
        store_git_summary: Arc::new(|store| keycord_git::password_store_git_state_summary(&store)),
        app_id: env!("APP_ID").to_string(),
    }
}

pub fn load_passwords_async(
    list: &ListBox,
    actions: &PasswordListActions,
    overlay: &ToastOverlay,
    should_show_list_actions: Rc<dyn Fn() -> bool>,
    show_hidden: bool,
    show_duplicates: bool,
) {
    keycord_entries::ui::list::load_passwords_async(
        list,
        actions,
        overlay,
        should_show_list_actions,
        show_hidden,
        show_duplicates,
        &entry_list_ports(),
    );
}

pub fn reload_password_list(
    list: &ListBox,
    actions: &PasswordListActions,
    overlay: &ToastOverlay,
    navigation: &NavigationView,
    visibility: &PasswordListVisibilityState,
) {
    load_passwords_async(
        list,
        actions,
        overlay,
        Rc::new({
            let navigation = navigation.clone();
            move || keycord_shell::ui::navigation_stack_is_root(&navigation)
        }),
        visibility.show_hidden(),
        visibility.show_duplicates(),
    );
    if let Some(root) = list.root() {
        if let Ok(window) = root.downcast::<ApplicationWindow>() {
            crate::window::sync_tools_action_availability(&window);
        }
    }
}

pub fn setup_search_filter(
    list: &ListBox,
    search_entry: &SearchEntry,
    header_focus_target: &Widget,
    placeholder_stack: &adw::gtk::Stack,
    placeholder_status: &adw::StatusPage,
    placeholder_spinner: &adw::gtk::Spinner,
    list_view: &adw::gtk::ScrolledWindow,
) {
    keycord_entries::ui::list::setup_search_filter(
        PasswordListSearchWidgets {
            list,
            search_entry,
            header_focus_target,
            placeholder_stack,
            placeholder_status,
            placeholder_spinner,
            list_view,
        },
        &entry_list_ports(),
    );
}

pub fn configure_password_list_store_filter(
    filter_button: &MenuButton,
    filter_popover: &Popover,
    filter_store_box: &GtkBox,
    list: &ListBox,
    navigation: &NavigationView,
    overlay: &ToastOverlay,
) {
    keycord_entries::ui::list::configure_password_list_store_filter(
        filter_button,
        filter_popover,
        filter_store_box,
        list,
        navigation,
        overlay,
        &entry_list_ports(),
    );
}

pub fn entry_tool_ports(refresh_tool_hub: Rc<dyn Fn()>) -> EntryToolUiPorts {
    EntryToolUiPorts {
        preferences: EntryToolPreferencesPorts {
            store_roots: Rc::new(|| Preferences::new().store_roots()),
            uses_integrated_backend: Rc::new(|| Preferences::new().uses_integrated_backend()),
        },
        backend: EntryToolBackendPorts {
            collect_entries: Arc::new(|options| {
                let preferences = Preferences::new();
                keycord_entries::model::collect_password_items_with_options(
                    &preferences.paths(),
                    preferences.password_list_sort_mode(),
                    options,
                    keycord_stores::recipients::store_is_supported_in_current_build,
                )
                .into_iter()
                .map(|entry| {
                    let label = entry.label();
                    EntryRequest {
                        root: entry.store_path,
                        label,
                    }
                })
                .collect()
            }),
            read_entry: Arc::new(|store, label| {
                crate::composition::backend::read_password_entry(&store, &label)
                    .map_err(|err| err.to_string())
            }),
            read_password_line: Arc::new(|store, label| {
                crate::composition::backend::read_password_line(&store, &label)
                    .map_err(|err| err.to_string())
            }),
        },
        keys: EntryToolKeyPorts {
            list_keys: Arc::new(|| {
                let mut keys = keycord_keys::list_ripasso_private_keys()?
                    .into_iter()
                    .map(|key| EntryToolKeySummary {
                        fingerprint: key.fingerprint,
                        user_ids: key.user_ids,
                    })
                    .collect::<Vec<_>>();
                keys.extend(
                    keycord_keys::list_connected_smartcard_keys()?
                        .into_iter()
                        .map(|key| EntryToolKeySummary {
                            fingerprint: key.fingerprint,
                            user_ids: key.user_ids,
                        }),
                );
                Ok(keys)
            }),
            requires_session_unlock: Arc::new(|fingerprint| {
                keycord_keys::ripasso_private_key_requires_session_unlock(&fingerprint)
            }),
            prompt_unlock: Rc::new(
                crate::composition::keys_unlock::prompt_private_key_unlock_for_action,
            ),
        },
        stores: EntryToolStorePorts {
            relevant_scopes: Arc::new(|store| {
                keycord_stores::recipients::relevant_store_recipient_scopes(&store)
            }),
            read_standard_recipients: Arc::new(|store, scope| {
                keycord_stores::recipients::read_store_standard_recipients_for_scope(&store, &scope)
            }),
            root_scope: keycord_stores::recipients::ROOT_STORE_RECIPIENTS_SCOPE.to_string(),
        },
        show_root_page: crate::composition::navigation::root_page_chrome_callback(),
        refresh_tool_hub,
    }
}

pub fn entry_page_ports() -> EntryPageUiPorts {
    EntryPageUiPorts {
        preferences: EntryPagePreferencesPorts {
            clear_empty_fields_before_save: Rc::new(|| {
                Preferences::new().clear_empty_fields_before_save()
            }),
            new_pass_file_template: Rc::new(|| Preferences::new().new_pass_file_template()),
            default_store: Rc::new(|| Preferences::new().store()),
            password_generation_settings: Rc::new(|| {
                Preferences::new().password_generation_settings()
            }),
            uses_integrated_backend: Rc::new(|| Preferences::new().uses_integrated_backend()),
            switch_to_integrated_backend: Rc::new(|| {
                Preferences::new()
                    .set_backend_kind(BackendKind::Integrated)
                    .map_err(|err| err.message.to_string())
            }),
            sync_private_keys_with_host: Rc::new(
                crate::composition::keys_sync::private_key_sync_enabled,
            ),
            disable_private_key_sync: Rc::new(|| {
                Preferences::new()
                    .set_sync_private_keys_with_host(false)
                    .map_err(|err| err.message.to_string())
            }),
        },
        backend: EntryPageBackendPorts {
            read_entry_with_progress: Arc::new(|store, label, progress_tx| {
                let mut report_progress = move |progress| {
                    let _ = progress_tx.send(progress);
                };
                crate::composition::backend::read_password_entry_with_progress(
                    &store,
                    &label,
                    &mut report_progress,
                )
            }),
            save_entry: Arc::new(|store, label, contents, overwrite| {
                crate::composition::backend::save_password_entry(
                    &store, &label, &contents, overwrite,
                )
            }),
            rename_entry: Arc::new(|store, old_label, new_label| {
                crate::composition::backend::rename_password_entry(&store, &old_label, &new_label)
            }),
        },
        open_preferences: Rc::new(|window| {
            keycord_shell::actions::activate_widget_action(window, "win.open-preferences");
        }),
        keys: EntryPageKeyPorts {
            preferred_fingerprint: Arc::new(|store, label| {
                keycord_stores::integrated_recipients::preferred_ripasso_private_key_fingerprint_for_entry(
                    &store,
                    &label,
                )
            }),
            prompt_unlock: Rc::new(
                crate::composition::keys_unlock::prompt_private_key_unlock_for_action,
            ),
            requires_passphrase: Arc::new(|bytes| {
                keycord_keys::ripasso_private_key_requires_passphrase(&bytes)
            }),
            import_private_key: Arc::new(|bytes, passphrase| {
                keycord_keys::import_ripasso_private_key_with_secret(&bytes, passphrase).map(|_| ())
            }),
            sync_to_host: Arc::new(|| {
                crate::composition::keys_sync::sync_private_keys_with_host(
                    keycord_keys::PrivateKeySyncDirection::AppToHost,
                )
            }),
            prompt_passphrase: Rc::new(|window, overlay, on_submit| {
                keycord_keys::ui::present_private_key_password_dialog(
                    window,
                    overlay,
                    "Unlock key",
                    None,
                    move |passphrase| on_submit(passphrase),
                );
            }),
        },
        prompt_git_unlock: Rc::new(|overlay, store, label, after_unlock| {
            crate::composition::git_signing::prompt_private_key_unlock_for_entry_git_commit_if_needed(
                overlay,
                &store,
                &label,
                &after_unlock,
            )
        }),
        list: entry_list_ports(),
        show_root_page: crate::composition::navigation::root_page_chrome_callback(),
        sync_tools_action_availability: Rc::new(crate::window::sync_tools_action_availability),
    }
}

#[cfg(test)]
mod tests {
    use super::delete_entry_with_optional_undo;
    use keycord_entries::model::PassEntry;
    use keycord_entries::undo::unavailable_undo_message;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_store(prefix: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    #[test]
    fn delete_entry_with_optional_undo_removes_invalid_files_without_git() {
        let store_root = temp_store("password-undo-invalid-delete");
        let entry_path = store_root.join("team/service.gpg");
        fs::create_dir_all(&store_root).expect("create store root");
        fs::create_dir_all(entry_path.parent().expect("entry parent")).expect("create entry dir");
        fs::write(&entry_path, b"not a valid password entry").expect("write invalid entry");

        let action = delete_entry_with_optional_undo(&PassEntry::from_label(
            store_root.to_string_lossy().to_string(),
            "team/service",
        ))
        .expect("delete invalid entry")
        .expect("record undo action");

        assert_eq!(
            unavailable_undo_message(&action),
            Some("Can't undo that change.")
        );
        assert!(!entry_path.exists());
        let _ = fs::remove_dir_all(store_root);
    }
}
