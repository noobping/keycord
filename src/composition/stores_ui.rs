//! Connects Stores UI ports to the selected application backends.

use adw::gio;
use adw::gtk::gdk::Display;
use adw::prelude::*;
use keycord_git::ui::{rebuild_store_recipients_git_row, StoreGitPageState};
use keycord_keys::ui::KeyManagementUiPorts;
use keycord_keys::HostGpgPrivateKeySummary;
use keycord_preferences::Preferences;
use keycord_shell::actions::activate_widget_action;
use keycord_stores::ui::ports::{
    StoreBackendUiPorts, StoreGitUiPorts, StoreImportUiPorts, StoreNavigationUiPorts,
    StorePassImportRequest, StorePreferencesPorts, StoreRefreshUiPorts, StoreUiPorts,
};
use keycord_stores::ROOT_STORE_RECIPIENTS_SCOPE;
use std::rc::Rc;
use std::sync::Arc;

pub fn store_preferences_ports() -> StorePreferencesPorts {
    StorePreferencesPorts::new(
        || Preferences::new().stores(),
        |stores| {
            Preferences::new()
                .set_stores(stores)
                .map_err(|err| err.to_string())
        },
        || {
            Preferences::new()
                .prune_missing_stores()
                .map_err(|err| err.to_string())
        },
        || Preferences::new().uses_integrated_backend(),
        || Preferences::new().uses_host_command_backend(),
    )
}

fn store_backend_ports() -> StoreBackendUiPorts {
    StoreBackendUiPorts {
        save_recipients: Arc::new(|store, scope, recipients, private_key_requirement| {
            if scope == ROOT_STORE_RECIPIENTS_SCOPE {
                crate::composition::backend::save_store_recipients(
                    &store,
                    &recipients,
                    private_key_requirement,
                )
            } else {
                crate::composition::backend::save_store_recipients_for_relative_dir(
                    &store,
                    &scope,
                    &recipients,
                    private_key_requirement,
                )
            }
        }),
        recipient_unlock_fingerprint: Arc::new(|store, scope| {
            if scope == ROOT_STORE_RECIPIENTS_SCOPE {
                crate::composition::backend::store_recipients_private_key_requiring_unlock(&store)
            } else {
                crate::composition::backend::store_recipients_private_key_requiring_unlock_for_relative_dir(
                    &store, &scope,
                )
            }
        }),
    }
}

fn store_git_ports(store_git_page: &StoreGitPageState) -> StoreGitUiPorts {
    let git_page = store_git_page.clone();
    StoreGitUiPorts {
        clone_repository: Arc::new(|url, store| {
            keycord_git::ui::clone_store_repository(&url, &store)
        }),
        rebuild_recipient_row: Rc::new(move |recipients_page| {
            rebuild_store_recipients_git_row(&git_page, recipients_page);
        }),
        prompt_recipient_commit_unlock: Rc::new(
            |overlay, store, recipients, requirement, after_unlock| {
                crate::composition::git_signing::prompt_private_key_unlock_for_store_git_commit_if_needed(
                    overlay,
                    store,
                    recipients,
                    requirement,
                    after_unlock,
                )
            },
        ),
    }
}

pub fn key_management_ui_ports() -> KeyManagementUiPorts {
    KeyManagementUiPorts {
        read_clipboard_text: Rc::new(|on_result| {
            let Some(display) = Display::default() else {
                on_result(Err("Clipboard unavailable.".to_string()));
                return;
            };
            display
                .clipboard()
                .read_text_async(None::<&gio::Cancellable>, move |result| {
                    on_result(
                        result
                            .map(|text| text.map(|text| text.to_string()))
                            .map_err(|err| err.to_string()),
                    );
                });
        }),
        list_host_private_keys: Arc::new(list_host_private_keys),
        copy_text: Rc::new(keycord_shell::clipboard::set_clipboard_text),
        prompt_unlock: Rc::new(
            crate::composition::keys_unlock::prompt_private_key_unlock_for_action,
        ),
        is_notice_hidden: Rc::new(|notice_id| Preferences::new().is_notice_hidden(notice_id)),
        hide_notice: Rc::new(|notice_id| {
            Preferences::new()
                .hide_notice(notice_id)
                .map_err(|err| err.to_string())
        }),
        private_key_sync_enabled: Rc::new(crate::composition::keys_sync::private_key_sync_enabled),
        disable_private_key_sync: Rc::new(|| {
            Preferences::new()
                .set_sync_private_keys_with_host(false)
                .map_err(|err| err.to_string())
        }),
        sync_private_keys_from_host: Arc::new(|| {
            crate::composition::keys_sync::sync_private_keys_with_host(
                keycord_keys::PrivateKeySyncDirection::HostToApp,
            )
        }),
        sync_private_keys_to_host: Arc::new(|| {
            crate::composition::keys_sync::sync_private_keys_with_host(
                keycord_keys::PrivateKeySyncDirection::AppToHost,
            )
        }),
        refresh_key_consumers: Rc::new(|window| {
            activate_widget_action(window, "win.reload-password-list");
        }),
        sync_optional_smartcard_access: Rc::new(
            crate::composition::host_access::append_optional_smartcard_access_group_row,
        ),
        #[cfg(feature = "fidokey")]
        sync_optional_fido_access: Rc::new(
            crate::composition::host_access::append_optional_fido2_access_group_row,
        ),
    }
}

fn list_host_private_keys() -> Result<Vec<HostGpgPrivateKeySummary>, String> {
    #[cfg(target_os = "linux")]
    {
        crate::composition::backend::host_gpg_backend().list_private_keys()
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(Vec::new())
    }
}

fn store_import_ports() -> StoreImportUiPorts {
    StoreImportUiPorts {
        available_sources: Arc::new(keycord_entries::import::available_pass_import_sources),
        run_import: Arc::new(|request: StorePassImportRequest| {
            keycord_entries::import::run_pass_import(&keycord_entries::import::PassImportRequest {
                store_root: request.store_root,
                source: request.source,
                source_path: request.source_path,
                source_password: request.source_password,
                target_path: request.target_path,
            })
        }),
    }
}

pub fn store_ui_ports(
    store_git_page: &StoreGitPageState,
    navigation: StoreNavigationUiPorts,
) -> StoreUiPorts {
    StoreUiPorts {
        preferences: store_preferences_ports(),
        backend: store_backend_ports(),
        git: store_git_ports(store_git_page),
        import: store_import_ports(),
        navigation,
        refresh: StoreRefreshUiPorts {
            reload_password_list: Rc::new(|window| {
                activate_widget_action(window, "win.reload-password-list");
            }),
            reload_store_recipients: Rc::new(|window| {
                activate_widget_action(window, "win.reload-store-recipients-list");
            }),
        },
    }
}
