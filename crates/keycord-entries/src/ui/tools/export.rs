use super::{EntryToolsState, FieldValueRequest};
use crate::model::CollectItemsOptions;
use crate::tools::{export_passwords_to_csv_with, unique_store_roots, EXPORT_FILE_NAME};
use adw::prelude::*;
use adw::{AlertDialog, ResponseAppearance, Toast};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_runtime::secure_fs::write_private_file;
use keycord_shell::background::spawn_result_task;
use keycord_shell::file_picker::choose_local_save_file_path;
use keycord_stores::labels::{shortened_store_label_for_path, shortened_store_label_map};
use std::path::Path;
use std::rc::Rc;

impl EntryToolsState {
    pub(super) fn connect_export_tool(&self) {
        let state = self.clone();
        self.select_page
            .export_row
            .connect_activated(move |_| state.confirm_password_export());
    }

    fn confirm_password_export(&self) {
        if self.advanced_search_tools_are_busy() {
            return;
        }

        let dialog = AlertDialog::builder()
            .heading(gettext("Export all passwords?"))
            .body(gettext("The CSV file will contain every password and field in plaintext. Anyone with access to the file can read them."))
            .build();
        let cancel = gettext("Cancel");
        let export = gettext("Export");
        dialog.add_responses(&[("cancel", cancel.as_str()), ("export", export.as_str())]);
        dialog.set_close_response("cancel");
        dialog.set_default_response(Some("cancel"));
        dialog.set_response_appearance("export", ResponseAppearance::Destructive);

        let state = self.clone();
        dialog.connect_response(Some("export"), move |_, _| {
            state.choose_password_export_path();
        });
        dialog.present(Some(&self.window));
    }

    fn choose_password_export_path(&self) {
        let state = self.clone();
        choose_local_save_file_path(
            &self.window,
            "Export passwords",
            "Export",
            EXPORT_FILE_NAME,
            ("CSV files", "csv"),
            &self.overlay,
            move |path| state.start_password_export(path),
        );
    }

    fn start_password_export(&self, path: String) {
        if self.advanced_search_tools_are_busy() {
            return;
        }

        self.set_export_tool_busy(true);
        let requests = self.export_entry_requests();
        self.unlock_tool_keys_if_needed(
            requests,
            Rc::new({
                let state = self.clone();
                move |requests| state.write_password_export(path.clone(), requests)
            }),
            Rc::new({
                let state = self.clone();
                move || state.set_export_tool_busy(false)
            }),
        );
    }

    fn write_password_export(&self, path: String, requests: Vec<FieldValueRequest>) {
        let state_for_result = self.clone();
        let state_for_disconnect = self.clone();
        let read_entry = self.ports.backend.read_entry.clone();
        spawn_result_task(
            move || export_passwords_to_csv(&path, requests, &read_entry),
            move |result| state_for_result.finish_password_export(result),
            move || {
                state_for_disconnect.set_export_tool_busy(false);
                state_for_disconnect
                    .overlay
                    .add_toast(Toast::new(&gettext("Couldn't export passwords.")));
            },
        );
    }

    fn finish_password_export(&self, result: Result<usize, String>) {
        self.set_export_tool_busy(false);
        match result {
            Ok(count) => {
                let message = if count == 1 {
                    gettext("Exported {count} password.")
                } else {
                    gettext("Exported {count} passwords.")
                }
                .replace("{count}", &count.to_string());
                self.overlay.add_toast(Toast::new(&message));
            }
            Err(err) => {
                log_error(err);
                self.overlay
                    .add_toast(Toast::new(&gettext("Couldn't export passwords.")));
            }
        }
    }

    fn export_entry_requests(&self) -> Vec<FieldValueRequest> {
        (self.ports.backend.collect_entries)(CollectItemsOptions {
            show_hidden: true,
            show_duplicates: false,
        })
    }
}

fn export_passwords_to_csv(
    path: &str,
    requests: Vec<FieldValueRequest>,
    read_entry: &super::ReadToolEntry,
) -> Result<usize, String> {
    let store_labels = shortened_store_label_map(&unique_store_roots(&requests));
    export_passwords_to_csv_with(
        requests,
        |root| shortened_store_label_for_path(root, &store_labels),
        |request| {
            read_entry(request.root.clone(), request.label.clone()).map_err(|err| {
                format!(
                    "Failed to read password entry '{}' from '{}': {err}",
                    request.label, request.root
                )
            })
        },
        |bytes| {
            write_private_file(Path::new(path), bytes)
                .map_err(|err| format!("Failed to write password export to '{path}': {err}"))
        },
    )
}
