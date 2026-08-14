use crate::model::OpenPassFile;
use crate::ui::list::{
    clear_password_search, password_list_row_action_kind, toggle_password_list_folder_row,
    PasswordListActionRowKind,
};
use crate::ui::new_item::{
    clear_new_password_dialog_error, selected_new_password_store, show_new_password_dialog_error,
    NewPasswordDialogState,
};
use crate::ui::page::{
    add_empty_otp_secret, add_pass_field_from_input, apply_pass_file_template,
    begin_new_password_entry, clean_pass_file, copy_current_otp, copy_current_password,
    copy_current_username, focus_add_pass_field_input, generate_password_entry,
    import_private_key_from_current_pass_file, open_password_entry_page,
    refresh_apply_template_button, refresh_password_analysis_label, save_current_password_entry,
    show_raw_pass_file_page, toggle_password_options, PasswordPageState,
};
use adw::glib::Propagation;
use adw::gtk::{gdk, Button, DirectionType, EventControllerKey, ListBox, PropagationPhase, Widget};
use adw::prelude::*;
use adw::{Application, EntryRow, PasswordEntryRow, Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_shell::actions::{activate_widget_action, register_window_action};
use keycord_shell::clipboard::connect_copy_and_qr_buttons;
use keycord_shell::object_data::non_null_to_string_option;
use keycord_shell::ui::connect_entry_row_apply_button_to_nonempty_text;

pub fn configure_password_save_button(button: &Button) {
    button.set_action_name(Some("win.save-password"));
    button.set_tooltip_text(Some(&gettext("Save")));
}

pub fn configure_entry_window_shortcuts(app: &Application) {
    app.set_accels_for_action("win.toggle-hidden-and-duplicates", &["<primary>h"]);
    app.set_accels_for_action("win.open-new-password", &["<primary>n"]);
    app.set_accels_for_action("win.open-raw-pass-file", &["<primary><shift>r"]);
    app.set_accels_for_action("win.copy-password", &["<primary><shift>c"]);
    app.set_accels_for_action("win.copy-username", &["<primary><shift>u"]);
    app.set_accels_for_action("win.copy-otp", &["<primary><shift>t"]);
    app.set_accels_for_action("win.apply-pass-template", &["<primary><shift>a"]);
    app.set_accels_for_action("win.add-pass-field", &["<primary><shift>f"]);
    app.set_accels_for_action("win.add-otp-secret", &["<primary><shift>o"]);
    app.set_accels_for_action("win.clean-pass-file", &["<primary><shift>k"]);
    app.set_accels_for_action("win.generate-password", &["<primary><shift>g"]);
    app.set_accels_for_action("win.toggle-password-options", &["<primary><shift>p"]);
}

pub fn connect_password_list_activation(
    list: &ListBox,
    search_entry: &adw::gtk::SearchEntry,
    overlay: &ToastOverlay,
    page_state: &PasswordPageState,
) {
    let search_entry = search_entry.clone();
    let overlay = overlay.clone();
    let page_state = page_state.clone();
    list.connect_row_activated(move |list, row| {
        if toggle_password_list_folder_row(list, row) {
            return;
        }

        match password_list_row_action_kind(row) {
            Some(PasswordListActionRowKind::NewPassword) => {
                activate_widget_action(row, "win.open-new-password");
                return;
            }
            Some(PasswordListActionRowKind::ClearSearch) => {
                clear_password_search(&search_entry, list);
                return;
            }
            None => {}
        }

        if matches!(
            non_null_to_string_option(row, "openable").as_deref(),
            Some("false")
        ) {
            return;
        }

        let label = non_null_to_string_option(row, "label");
        let root = non_null_to_string_option(row, "root");

        let Some(label) = label else {
            overlay.add_toast(Toast::new(&gettext("Couldn't open that item.")));
            return;
        };
        let Some(root) = root else {
            overlay.add_toast(Toast::new(&gettext("That item is missing its store.")));
            return;
        };
        let opened_pass_file = OpenPassFile::from_label(root, &label);
        open_password_entry_page(&page_state, opened_pass_file, true);
    });
}

pub fn connect_password_copy_buttons(
    overlay: &ToastOverlay,
    password: (&PasswordEntryRow, &Button, &Button),
    username: (&EntryRow, &Button, &Button),
    otp: (&PasswordEntryRow, &Button, &Button),
) {
    {
        let entry = password.0.clone();
        let button = password.1.clone();
        connect_copy_and_qr_buttons(&button, password.2, overlay, move || {
            entry.text().to_string()
        });
    }
    {
        let entry = username.0.clone();
        let button = username.1.clone();
        connect_copy_and_qr_buttons(&button, username.2, overlay, move || {
            entry.text().to_string()
        });
    }
    {
        let entry = otp.0.clone();
        let button = otp.1.clone();
        connect_copy_and_qr_buttons(&button, otp.2, overlay, move || entry.text().to_string());
    }
}

pub fn connect_new_password_submit(
    page_state: &PasswordPageState,
    dialog_state: &NewPasswordDialogState,
) {
    let page_state_for_apply = page_state.clone();
    let dialog_state_for_apply = dialog_state.clone();
    let path_entry = dialog_state_for_apply.path_entry.clone();
    path_entry.connect_apply(move |_| {
        clear_new_password_dialog_error(&dialog_state_for_apply);
        if let Err(message) = begin_new_password_entry(
            &page_state_for_apply,
            &dialog_state_for_apply.path_entry.text(),
            selected_new_password_store(&dialog_state_for_apply),
            &dialog_state_for_apply.dialog,
        ) {
            show_new_password_dialog_error(&dialog_state_for_apply, message);
        }
    });
}

pub fn register_password_page_actions(
    window: &adw::ApplicationWindow,
    page_state: &PasswordPageState,
) {
    {
        let page_state = page_state.clone();
        let page = page_state.page.clone();
        let page_for_keys = page.clone();
        let template_button: Widget = page_state.template_button.clone().upcast();
        let clean_button: Widget = page_state.clean_button.clone().upcast();
        let otp_add_button: Widget = page_state.otp_add_button.clone().upcast();
        let import_private_key_button: Widget =
            page_state.import_private_key_button.clone().upcast();
        let editor_save_button: Widget = page_state.editor_save_button.clone().upcast();
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(PropagationPhase::Capture);
        controller.connect_key_pressed(move |_, key, _, _| {
            let direction = match key {
                gdk::Key::Up | gdk::Key::KP_Up => DirectionType::Up,
                gdk::Key::Down | gdk::Key::KP_Down => DirectionType::Down,
                _ => return Propagation::Proceed,
            };

            let Some(root) = page_for_keys.root() else {
                return Propagation::Proceed;
            };
            let Some(focus) = adw::gtk::prelude::RootExt::focus(&root) else {
                return Propagation::Proceed;
            };
            if !focus.is::<Button>() || !focus.is_ancestor(&page_for_keys) {
                return Propagation::Proceed;
            }

            if matches!(direction, DirectionType::Up)
                && (focus == template_button
                    || focus == clean_button
                    || focus == otp_add_button
                    || focus == import_private_key_button
                    || focus == editor_save_button)
            {
                focus_add_pass_field_input(&page_state);
                return Propagation::Stop;
            }

            if page_for_keys.child_focus(direction) {
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        });
        page.add_controller(controller);
    }

    {
        let page_state = page_state.clone();
        let buffer = page_state.text.buffer();
        buffer.connect_changed(move |_| {
            refresh_apply_template_button(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        let password_entry = page_state.entry.clone();
        password_entry.connect_changed(move |_| {
            refresh_password_analysis_label(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        let add_field_row = page_state.field_add_row.clone();
        connect_entry_row_apply_button_to_nonempty_text(&add_field_row);
        add_field_row.connect_apply(move |_| {
            add_pass_field_from_input(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "save-password", move || {
            save_current_password_entry(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "open-raw-pass-file", move || {
            show_raw_pass_file_page(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "add-otp-secret", move || {
            add_empty_otp_secret(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "add-pass-field", move || {
            focus_add_pass_field_input(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "clean-pass-file", move || {
            clean_pass_file(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "apply-pass-template", move || {
            apply_pass_file_template(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "generate-password", move || {
            generate_password_entry(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "import-private-key-from-pass-file", move || {
            import_private_key_from_current_pass_file(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "copy-password", move || {
            copy_current_password(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "copy-username", move || {
            copy_current_username(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "copy-otp", move || {
            copy_current_otp(&page_state);
        });
    }

    {
        let page_state = page_state.clone();
        register_window_action(window, "toggle-password-options", move || {
            toggle_password_options(&page_state);
        });
    }
}
