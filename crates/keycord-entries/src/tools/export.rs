use super::EntryRequest;
use crate::file::{is_passkey_storage_line, parse_structured_pass_lines, StructuredPassLine};
use zeroize::{Zeroize, Zeroizing};

pub const EXPORT_FILE_NAME: &str = "keycord-passwords.csv";
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
    fn from_contents(store: String, request: &EntryRequest, contents: &str) -> Self {
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

pub fn export_passwords_to_csv_with(
    requests: Vec<EntryRequest>,
    mut store_label: impl FnMut(&str) -> String,
    mut read_entry: impl FnMut(&EntryRequest) -> Result<String, String>,
    mut write_export: impl FnMut(&[u8]) -> Result<(), String>,
) -> Result<usize, String> {
    let mut csv = Zeroizing::new(String::new());
    append_csv_record(&mut csv, CSV_HEADER);

    let count = requests.len();
    for request in requests {
        let contents = Zeroizing::new(read_entry(&request)?);
        let mut row = CsvExportRow::from_contents(store_label(&request.root), &request, &contents);
        append_csv_record(&mut csv, row.fields());
        row.zeroize();
    }

    write_export(csv.as_bytes())?;
    Ok(count)
}

pub fn unique_store_roots(requests: &[EntryRequest]) -> Vec<String> {
    let mut roots = Vec::new();
    for request in requests {
        if !roots.contains(&request.root) {
            roots.push(request.root.clone());
        }
    }
    roots
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
        append_csv_record, export_passwords_to_csv_with, redacted_export_contents, CsvExportRow,
        EntryRequest, CSV_HEADER,
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
        let request = EntryRequest {
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
    fn passkey_material_is_redacted() {
        let contents =
            "\npasskey: {\"type\":\"passkey\",\"key\":\"private-material\"}\nurl: example.com";
        let redacted = redacted_export_contents(contents);
        assert_eq!(redacted, "\npasskey: [redacted]\nurl: example.com");
        assert!(!redacted.contains("private-material"));
    }

    #[test]
    fn export_engine_uses_injected_io() {
        let requests = vec![EntryRequest {
            root: "/stores/main".to_string(),
            label: "team/service".to_string(),
        }];
        let mut written = Vec::new();
        let count = export_passwords_to_csv_with(
            requests,
            |_| "main".to_string(),
            |_| Ok("secret\nusername: alice".to_string()),
            |bytes| {
                written.extend_from_slice(bytes);
                Ok(())
            },
        )
        .expect("export");
        assert_eq!(count, 1);
        assert!(String::from_utf8(written)
            .expect("utf8")
            .starts_with("\"store\",\"store_path\",\"entry\""));
    }

    #[test]
    fn csv_header_describes_every_column() {
        let mut output = String::new();
        append_csv_record(&mut output, CSV_HEADER);
        assert_eq!(output.matches(',').count(), 8);
    }
}
