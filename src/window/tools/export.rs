use super::{FieldValueRequest, ToolsPageState};
use crate::backend::read_password_entry;
use crate::i18n::gettext;
use crate::logging::log_error;
use crate::password::file::{
    is_passkey_storage_line, parse_structured_pass_lines, StructuredPassLine,
};
use crate::password::model::{collect_all_password_items_with_options, CollectItemsOptions};
use crate::store::labels::{shortened_store_label_for_path, shortened_store_label_map};
use crate::support::background::spawn_result_task;
use crate::support::file_picker::choose_local_save_file_path;
use crate::support::secure_fs::write_private_file;
use adw::prelude::*;
use adw::{AlertDialog, ResponseAppearance, Toast};
use std::path::Path;
use std::rc::Rc;
use zeroize::{Zeroize, Zeroizing};

const EXPORT_FILE_NAME: &str = "keycord-passwords.csv";
const CSV_HEADER: [&str; 9] = [
    "store",
    "store_path",
    "entry",
    "password",
    "username",
    "otp",
    "fields",
    "notes",
    "contents",
];

struct CsvExportRow {
    store: String,
    store_path: String,
    entry: String,
    password: String,
    username: String,
    otp: String,
    fields: String,
    notes: String,
    contents: String,
}

impl CsvExportRow {
    fn from_contents(store: String, request: &FieldValueRequest, contents: &str) -> Self {
        let (password, structured_lines) = parse_structured_pass_lines(contents);
        let mut usernames = Vec::new();
        let mut otp_urls = Vec::new();
        let mut fields = Vec::new();
        let mut notes = Vec::new();

        for ((line, value), raw_line) in structured_lines.into_iter().zip(contents.lines().skip(1))
        {
            if is_passkey_storage_line(raw_line) {
                continue;
            }
            match line {
                StructuredPassLine::Username(_) => {
                    if let Some(value) = value {
                        usernames.push(value);
                    }
                }
                StructuredPassLine::Otp(_) => {
                    if let Some(value) = value {
                        otp_urls.push(value);
                    }
                }
                StructuredPassLine::Passkey(_) => {}
                StructuredPassLine::Field(_) => fields.push(raw_line.to_string()),
                StructuredPassLine::Preserved(_) => notes.push(raw_line.to_string()),
            }
        }

        Self {
            store,
            store_path: request.root.clone(),
            entry: request.label.clone(),
            password,
            username: usernames.join("\n"),
            otp: otp_urls.join("\n"),
            fields: fields.join("\n"),
            notes: notes.join("\n"),
            contents: redacted_export_contents(contents),
        }
    }

    fn fields(&self) -> [&str; 9] {
        [
            &self.store,
            &self.store_path,
            &self.entry,
            &self.password,
            &self.username,
            &self.otp,
            &self.fields,
            &self.notes,
            &self.contents,
        ]
    }

    fn zeroize(&mut self) {
        self.store.zeroize();
        self.store_path.zeroize();
        self.entry.zeroize();
        self.password.zeroize();
        self.username.zeroize();
        self.otp.zeroize();
        self.fields.zeroize();
        self.notes.zeroize();
        self.contents.zeroize();
    }
}

fn redacted_export_contents(contents: &str) -> String {
    if !contents.lines().any(is_passkey_storage_line) {
        return contents.to_string();
    }

    let mut redacted = String::with_capacity(contents.len());
    for segment in contents.split_inclusive('\n') {
        let (line_with_cr, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |line| (line, "\n"));
        let (line, carriage_return) = line_with_cr
            .strip_suffix('\r')
            .map_or((line_with_cr, ""), |line| (line, "\r"));
        if is_passkey_storage_line(line) {
            redacted.push_str("passkey: [redacted]");
            redacted.push_str(carriage_return);
            redacted.push_str(newline);
        } else {
            redacted.push_str(segment);
        }
    }
    redacted
}

impl ToolsPageState {
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
            "csv",
            &self.overlay,
            move |path| state.start_password_export(path),
        );
    }

    fn start_password_export(&self, path: String) {
        if self.advanced_search_tools_are_busy() {
            return;
        }

        self.set_export_tool_busy(true);
        let requests = export_entry_requests();
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
        spawn_result_task(
            move || export_passwords_to_csv(&path, requests),
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
}

fn export_entry_requests() -> Vec<FieldValueRequest> {
    collect_all_password_items_with_options(CollectItemsOptions {
        show_hidden: true,
        show_duplicates: false,
    })
    .into_iter()
    .map(|entry| {
        let label = entry.label();
        FieldValueRequest {
            root: entry.store_path,
            label,
        }
    })
    .collect()
}

fn export_passwords_to_csv(path: &str, requests: Vec<FieldValueRequest>) -> Result<usize, String> {
    let store_roots = unique_store_roots(&requests);
    let store_labels = shortened_store_label_map(&store_roots);
    let mut csv = Zeroizing::new(String::new());
    append_csv_record(&mut csv, CSV_HEADER);

    let count = requests.len();
    for request in requests {
        let contents = Zeroizing::new(read_password_entry(&request.root, &request.label).map_err(
            |err| {
                format!(
                    "Failed to read password entry '{}' from '{}': {err}",
                    request.label, request.root
                )
            },
        )?);
        let store = shortened_store_label_for_path(&request.root, &store_labels);
        let mut row = CsvExportRow::from_contents(store, &request, &contents);
        append_csv_record(&mut csv, row.fields());
        row.zeroize();
    }

    write_private_file(Path::new(path), csv.as_bytes())
        .map_err(|err| format!("Failed to write password export to '{path}': {err}"))?;
    Ok(count)
}

fn unique_store_roots(requests: &[FieldValueRequest]) -> Vec<String> {
    let mut roots = Vec::new();
    for request in requests {
        if !roots.contains(&request.root) {
            roots.push(request.root.clone());
        }
    }
    roots
}

fn append_csv_record<'a>(output: &mut String, fields: impl IntoIterator<Item = &'a str>) {
    for (index, field) in fields.into_iter().enumerate() {
        if index > 0 {
            output.push(',');
        }
        output.push('"');
        for character in field.chars() {
            if character == '"' {
                output.push('"');
            }
            output.push(character);
        }
        output.push('"');
    }
    output.push_str("\r\n");
}

#[cfg(test)]
mod tests {
    use super::{
        append_csv_record, redacted_export_contents, CsvExportRow, FieldValueRequest, CSV_HEADER,
    };

    #[test]
    fn csv_records_quote_commas_quotes_and_newlines() {
        let mut output = String::new();
        append_csv_record(
            &mut output,
            ["plain", "comma,value", "say \"hello\"", "two\nlines"],
        );

        assert_eq!(
            output,
            "\"plain\",\"comma,value\",\"say \"\"hello\"\"\",\"two\nlines\"\r\n"
        );
    }

    #[test]
    fn export_row_preserves_structured_fields_notes_and_raw_contents() {
        let request = FieldValueRequest {
            root: "/stores/main".to_string(),
            label: "team/service".to_string(),
        };
        let contents = "s3cret\nlogin: alice\notpauth://totp/Test?secret=ABC\nurl: https://example.com\na note";
        let mut row = CsvExportRow::from_contents("main".to_string(), &request, contents);

        assert_eq!(
            row.fields(),
            [
                "main",
                "/stores/main",
                "team/service",
                "s3cret",
                "alice",
                "otpauth://totp/Test?secret=ABC",
                "url: https://example.com",
                "a note",
                contents,
            ]
        );
        row.zeroize();
    }

    #[test]
    fn csv_header_describes_every_exported_column() {
        let mut output = String::new();
        append_csv_record(&mut output, CSV_HEADER);

        assert_eq!(output.matches(',').count(), 8);
        assert!(output.starts_with("\"store\",\"store_path\",\"entry\""));
        assert!(output.ends_with("\"contents\"\r\n"));
    }

    #[test]
    fn raw_passkey_fields_are_redacted_from_password_csv_exports() {
        let contents =
            "\npasskey: {\"type\":\"passkey\",\"key\":\"private-material\"}\nurl: example.com";
        let redacted = redacted_export_contents(contents);

        assert_eq!(
            redacted,
            "\npasskey: [redacted]\nurl: example.com".to_string()
        );
        assert!(!redacted.contains("private-material"));

        let request = FieldValueRequest {
            root: "/stores/main".to_string(),
            label: "example/alice".to_string(),
        };
        let row = CsvExportRow::from_contents("main".to_string(), &request, contents);
        assert!(row
            .fields()
            .iter()
            .all(|field| !field.contains("private-material")));
        assert!(row.notes.is_empty());
        assert_eq!(row.fields, "url: example.com");
    }

    #[test]
    fn export_redaction_preserves_existing_line_endings_and_final_newlines() {
        let ordinary = "secret\r\nurl: example.com\r\n";
        assert_eq!(redacted_export_contents(ordinary), ordinary);

        let passkey =
            "\r\npasskey: {\"type\":\"passkey\",\"key\":\"secret\"}\r\nurl: example.com\r\n";
        assert_eq!(
            redacted_export_contents(passkey),
            "\r\npasskey: [redacted]\r\nurl: example.com\r\n"
        );

        let noncanonical = "\nPassKey : private JSON\n\n";
        assert_eq!(
            redacted_export_contents(noncanonical),
            "\npasskey: [redacted]\n\n"
        );
    }
}
