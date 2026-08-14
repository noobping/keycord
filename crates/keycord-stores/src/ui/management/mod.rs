mod clone;
mod import;

use self::clone::append_store_clone_row;
#[cfg(target_os = "linux")]
pub use self::clone::prompt_store_clone;
pub use self::import::{
    initialize_store_import_page, schedule_store_import_row, StoreImportPageState,
    StoreImportToolRowState,
};
pub use super::recipient_page::{
    connect_store_recipients_controls, register_store_recipients_reload_action,
    register_store_recipients_save_action, show_store_recipients_create_page,
    show_store_recipients_edit_page, sync_store_recipients_page_header, StoreRecipientsPageState,
    StoreRecipientsPlatformState, StoreRecipientsRequest,
};
pub use crate::management::NUMBERED_STORE_SHORTCUT_COUNT;
use crate::management::{
    configured_store_for_shortcut_slot, empty_store_list_text, folder_is_empty,
    initial_recipients_for_store_creation, selected_store_folder_mode, updated_stores_after_add,
    updated_stores_after_delete, SelectedStoreFolderMode,
};
use crate::recipients::{
    read_store_recipients, store_is_supported_in_current_build, store_recipients_subtitle,
};
use adw::gtk::ListBox;
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::actions::register_window_action;
use keycord_shell::file_picker::choose_local_folder_path;
use keycord_shell::ui::{
    append_action_row_with_button, append_info_row, clear_list_box, dim_label_icon,
    flat_icon_button,
};
use std::rc::Rc;

fn configured_store_for_shortcut(state: &StoreRecipientsPageState, slot: usize) -> Option<String> {
    configured_store_for_shortcut_slot(&state.ports.preferences.stores(), slot)
}

fn open_store_folder_picker(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    create_folders: bool,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    choose_local_folder_path(
        window,
        title,
        accept_label,
        create_folders,
        overlay,
        on_selected,
    );
}

pub fn rebuild_store_list(
    stores_list: &ListBox,
    actions_list: &ListBox,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
    before_navigation: Option<Rc<dyn Fn()>>,
) {
    match recipients_page.ports.preferences.prune_missing_stores() {
        Ok(true) => refresh_after_store_list_change(recipients_page),
        Ok(false) => {}
        Err(err) => {
            log_error(format!("Failed to remove missing password stores: {err}"));
        }
    }

    rebuild_stores_list(stores_list, recipients_page, before_navigation.clone());
    rebuild_store_actions_list(
        actions_list,
        stores_list,
        window,
        overlay,
        recipients_page,
        before_navigation,
    );
}

pub fn refresh_after_store_list_change(recipients_page: &StoreRecipientsPageState) {
    (recipients_page.ports.refresh.reload_password_list)(&recipients_page.window);
    (recipients_page.ports.refresh.reload_store_recipients)(&recipients_page.window);
}

pub fn rebuild_stores_list(
    stores_list: &ListBox,
    recipients_page: &StoreRecipientsPageState,
    before_navigation: Option<Rc<dyn Fn()>>,
) {
    clear_list_box(stores_list);

    let stores = recipients_page.ports.preferences.stores();
    if stores.is_empty() {
        append_empty_store_list_row(stores_list);
        return;
    }

    for store in &stores {
        append_store_row(
            stores_list,
            store,
            recipients_page,
            before_navigation.clone(),
        );
    }
}

fn append_empty_store_list_row(list: &ListBox) {
    let (title, subtitle) = empty_store_list_text();
    append_info_row(list, title, subtitle);
}

pub fn rebuild_store_actions_list(
    actions_list: &ListBox,
    stores_list: &ListBox,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
    before_navigation: Option<Rc<dyn Fn()>>,
) {
    clear_list_box(actions_list);

    append_store_picker_row(
        actions_list,
        stores_list,
        window,
        overlay,
        recipients_page,
        before_navigation.clone(),
    );
    append_store_clone_row(
        actions_list,
        stores_list,
        window,
        overlay,
        recipients_page,
        before_navigation,
    );
}

fn append_store_row(
    list: &ListBox,
    store: &str,
    recipients_page: &StoreRecipientsPageState,
    before_navigation: Option<Rc<dyn Fn()>>,
) {
    let store_supported = store_is_supported_in_current_build(store);
    let row = ActionRow::builder()
        .title(store)
        .subtitle(store_recipients_subtitle(store))
        .build();
    row.set_activatable(store_supported);

    if store_supported {
        row.add_suffix(&dim_label_icon("go-next-symbolic"));
    }
    if !store_supported {
        row.add_prefix(&dim_label_icon("dialog-warning-symbolic"));
    }

    let delete_button = flat_icon_button("window-close-symbolic");
    row.add_suffix(&delete_button);

    list.append(&row);

    let list = list.clone();
    let store = store.to_string();
    let recipients_page_for_edit = recipients_page.clone();
    let recipients_page_for_delete = recipients_page.clone();
    let store_for_edit = store.clone();
    let before_navigation_for_edit = before_navigation.clone();

    row.connect_activated(move |_| {
        if let Some(before_navigation) = &before_navigation_for_edit {
            before_navigation();
        }
        show_store_recipients_edit_page(&recipients_page_for_edit, &store_for_edit);
    });

    delete_button.connect_clicked(move |_| {
        let preferences = &recipients_page_for_delete.ports.preferences;
        if let Some(stores) = updated_stores_after_delete(&preferences.stores(), &store) {
            if let Err(err) = preferences.set_stores(stores) {
                log_error(format!("Failed to save stores: {err}"));
            } else {
                rebuild_stores_list(
                    &list,
                    &recipients_page_for_delete,
                    before_navigation.clone(),
                );
                refresh_after_store_list_change(&recipients_page_for_delete);
            }
        }
    });
}

fn append_store_picker_row(
    list: &ListBox,
    stores_list: &ListBox,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
    before_navigation: Option<Rc<dyn Fn()>>,
) {
    let window = window.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    let stores_list_for_action = stores_list.clone();
    let before_navigation_for_action = before_navigation.clone();
    append_action_row_with_button(
        list,
        "Add or create store",
        "Choose a folder. If it is empty, it becomes a store.",
        "folder-new-symbolic",
        move || {
            if let Some(before_navigation) = &before_navigation_for_action {
                before_navigation();
            }
            prompt_add_or_create_store(
                &window,
                &stores_list_for_action,
                &overlay,
                &recipients_page,
                before_navigation.clone(),
            );
        },
    );
}

pub fn prompt_add_or_create_store(
    window: &ApplicationWindow,
    stores_list: &ListBox,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
    before_navigation: Option<Rc<dyn Fn()>>,
) {
    let stores_list = stores_list.clone();
    let window = window.clone();
    let overlay = overlay.clone();
    let overlay_for_selection = overlay.clone();
    let recipients_page = recipients_page.clone();
    open_store_folder_picker(
        &window,
        "Choose store folder",
        "Select",
        true,
        &overlay,
        move |store| {
            let mode = match folder_is_empty(&store) {
                Ok(is_empty) => selected_store_folder_mode(is_empty),
                Err(err) => {
                    log_error(format!("Failed to read password store folder: {err}"));
                    overlay_for_selection
                        .add_toast(Toast::new(&gettext("Couldn't read that folder.")));
                    return;
                }
            };

            match mode {
                SelectedStoreFolderMode::AddExisting => {
                    let mut store_added = false;
                    let preferences = &recipients_page.ports.preferences;
                    if let Some(stores) = updated_stores_after_add(&preferences.stores(), &store) {
                        if let Err(err) = preferences.set_stores(stores) {
                            log_error(format!("Failed to save stores: {err}"));
                            overlay_for_selection
                                .add_toast(Toast::new(&gettext("Couldn't add that folder.")));
                            return;
                        }
                        store_added = true;
                    }

                    rebuild_stores_list(&stores_list, &recipients_page, before_navigation.clone());
                    show_store_recipients_edit_page(&recipients_page, &store);
                    if store_added {
                        refresh_after_store_list_change(&recipients_page);
                    }
                }
                SelectedStoreFolderMode::CreateNew => {
                    let recipients =
                        initial_recipients_for_store_creation(read_store_recipients(&store));
                    show_store_recipients_create_page(&recipients_page, store, recipients);
                }
            }
        },
    );
}

pub fn register_open_store_picker_action(
    window: &ApplicationWindow,
    stores_list: &ListBox,
    overlay: &ToastOverlay,
    recipients_page: &StoreRecipientsPageState,
) {
    let action_window = window.clone();
    let prompt_window = action_window.clone();
    let stores_list = stores_list.clone();
    let overlay = overlay.clone();
    let recipients_page = recipients_page.clone();
    register_window_action(&action_window, "open-store-picker", move || {
        prompt_add_or_create_store(
            &prompt_window,
            &stores_list,
            &overlay,
            &recipients_page,
            None,
        );
    });
}

pub fn register_open_store_recipients_shortcut_actions(
    window: &ApplicationWindow,
    recipients_page: &StoreRecipientsPageState,
) {
    for slot in 1..=NUMBERED_STORE_SHORTCUT_COUNT {
        let action_window = window.clone();
        let recipients_page = recipients_page.clone();
        register_window_action(
            &action_window,
            &format!("open-store-recipients-{slot}"),
            move || {
                let Some(store) = configured_store_for_shortcut(&recipients_page, slot) else {
                    return;
                };

                show_store_recipients_edit_page(&recipients_page, store);
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{
        configured_store_for_shortcut_slot, empty_store_list_text,
        initial_recipients_for_store_creation, selected_store_folder_mode,
        updated_stores_after_add, updated_stores_after_delete, SelectedStoreFolderMode,
    };

    #[test]
    fn adding_a_new_store_appends_it_once() {
        let stores = vec!["/tmp/one".to_string()];

        assert_eq!(
            updated_stores_after_add(&stores, "/tmp/two"),
            Some(vec!["/tmp/one".to_string(), "/tmp/two".to_string()])
        );
        assert_eq!(updated_stores_after_add(&stores, "/tmp/one"), None);
    }

    #[test]
    fn deleting_a_store_removes_only_the_requested_entry() {
        let stores = vec![
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
            "/tmp/three".to_string(),
        ];

        assert_eq!(
            updated_stores_after_delete(&stores, "/tmp/two"),
            Some(vec!["/tmp/one".to_string(), "/tmp/three".to_string()])
        );
        assert_eq!(updated_stores_after_delete(&stores, "/tmp/missing"), None);
    }

    #[test]
    fn store_creation_starts_empty_unless_the_folder_already_has_recipients() {
        assert_eq!(
            initial_recipients_for_store_creation(vec!["existing@example.com".to_string()]),
            vec!["existing@example.com".to_string()]
        );
        assert_eq!(
            initial_recipients_for_store_creation(Vec::new()),
            Vec::<String>::new()
        );
    }

    #[test]
    fn empty_selected_folders_start_store_creation_while_non_empty_ones_are_added() {
        assert_eq!(
            selected_store_folder_mode(true),
            SelectedStoreFolderMode::CreateNew
        );
        assert_eq!(
            selected_store_folder_mode(false),
            SelectedStoreFolderMode::AddExisting
        );
    }

    #[test]
    fn empty_store_list_has_text() {
        assert_eq!(
            empty_store_list_text(),
            ("No password stores", "Add a folder.")
        );
    }

    #[test]
    fn numbered_store_shortcuts_follow_the_first_six_configured_stores() {
        let stores = vec![
            "/tmp/one".to_string(),
            "/tmp/two".to_string(),
            "/tmp/three".to_string(),
            "/tmp/four".to_string(),
            "/tmp/five".to_string(),
            "/tmp/six".to_string(),
            "/tmp/seven".to_string(),
        ];

        assert_eq!(
            configured_store_for_shortcut_slot(&stores, 1),
            Some("/tmp/one".to_string())
        );
        assert_eq!(
            configured_store_for_shortcut_slot(&stores, 6),
            Some("/tmp/six".to_string())
        );
        assert_eq!(configured_store_for_shortcut_slot(&stores, 0), None);
        assert_eq!(configured_store_for_shortcut_slot(&stores, 7), None);
    }
}
