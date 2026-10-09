#[cfg(feature = "passkey")]
use super::types::PasskeyLine;
use super::types::{
    is_otpauth_line, is_sensitive_field, is_username_field_key, DynamicFieldTemplate,
    OtpFieldTemplate, StructuredPassLine, UsernameFieldTemplate,
};
#[cfg(feature = "passkey")]
use keycord_passkey::inspect_passkey_storage_value;
#[cfg(feature = "passkey")]
use zeroize::Zeroizing;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchablePassField {
    pub key: String,
    pub value: String,
    pub normalized_value: String,
}

pub fn structured_username_value(lines: &[(StructuredPassLine, Option<String>)]) -> Option<String> {
    lines.iter().find_map(|(line, value)| match line {
        StructuredPassLine::Username(_) => value.clone(),
        _ => None,
    })
}

pub fn structured_otp_line(
    lines: &[(StructuredPassLine, Option<String>)],
) -> Option<(OtpFieldTemplate, String)> {
    lines.iter().find_map(|(line, value)| match line {
        StructuredPassLine::Otp(template) => value.clone().map(|url| (template.clone(), url)),
        _ => None,
    })
}

pub fn pass_file_has_otp(contents: &str) -> bool {
    let (_, structured_lines) = parse_structured_pass_lines(contents);
    structured_otp_line(&structured_lines).is_some()
}

/// Disabled-feature builds deliberately perform no passkey recognition.
pub fn pass_file_has_passkey(contents: &str) -> bool {
    #[cfg(feature = "passkey")]
    {
        inspect_passkey_storage_value(contents.lines().next().unwrap_or_default()).is_some()
    }
    #[cfg(not(feature = "passkey"))]
    {
        let _ = contents;
        false
    }
}

/// One shared gate for password-only operations in both backends.
pub fn password_line(contents: &str) -> Result<String, crate::PasswordEntryError> {
    if pass_file_has_passkey(contents) {
        return Err(crate::PasswordEntryError::other(
            "This entry contains a passkey, not a password.",
        ));
    }
    Ok(contents.lines().next().unwrap_or_default().to_string())
}

#[cfg(feature = "passkey")]
pub fn validate_passkey_path(contents: &str, label: &str) -> Result<(), String> {
    if let Some(credential) =
        inspect_passkey_storage_value(contents.lines().next().unwrap_or_default())
    {
        keycord_passkey::validate_passkey_entry_label(&credential?, label)?;
    }
    Ok(())
}

/// Text-only consumers must never receive native binary passkeys.
pub fn entry_text(bytes: &[u8]) -> Result<String, crate::PasswordEntryError> {
    #[cfg(feature = "passless")]
    if keycord_passkey::inspect_passless(bytes).is_some() {
        return Err(crate::PasswordEntryError::other(
            "This entry contains a passkey, not a password.",
        ));
    }
    std::str::from_utf8(bytes)
        .map(str::to_string)
        .map_err(|_| crate::PasswordEntryError::other("The entry is not UTF-8 text."))
}

pub fn validate_entry_bytes_path(bytes: &[u8], label: &str) -> Result<(), String> {
    #[cfg(feature = "passless")]
    if let Some(credential) = keycord_passkey::inspect_passless(bytes) {
        return credential?.validate_label(label);
    }
    #[cfg(feature = "passkey")]
    if let Ok(text) = std::str::from_utf8(bytes) {
        validate_passkey_path(text, label)?;
    }
    let _ = (bytes, label);
    Ok(())
}

pub fn canonical_search_field_key(key: &str) -> Option<String> {
    let key = key.trim();
    if key.is_empty() {
        return None;
    }

    if is_username_field_key(key) {
        return Some("username".to_string());
    }
    if key.eq_ignore_ascii_case("otpauth") {
        return None;
    }

    Some(key.to_ascii_lowercase())
}

pub fn searchable_pass_fields(contents: &str) -> Vec<SearchablePassField> {
    let (_, structured_lines) = parse_structured_pass_lines(contents);
    structured_lines
        .into_iter()
        .filter_map(|(line, value)| {
            let value = value?;
            let key = match line {
                StructuredPassLine::Username(_) => Some("username".to_string()),
                StructuredPassLine::Otp(_) => None,
                #[cfg(feature = "passkey")]
                StructuredPassLine::Passkey(_) => None,
                StructuredPassLine::Field(template) => canonical_search_field_key(&template.title),
                StructuredPassLine::Preserved(_) => None,
            }?;
            let normalized_value = value.to_lowercase();

            Some(SearchablePassField {
                key,
                value,
                normalized_value,
            })
        })
        .collect()
}

pub fn parse_structured_pass_lines(
    contents: &str,
) -> (String, Vec<(StructuredPassLine, Option<String>)>) {
    let mut lines = contents.lines();
    let password = lines.next().unwrap_or_default().to_string();
    let mut primary = Vec::new();
    #[cfg(feature = "passkey")]
    let password = if let Some(credential) = inspect_passkey_storage_value(&password) {
        primary.push((
            StructuredPassLine::Passkey(PasskeyLine {
                storage_value: Zeroizing::new(password),
                credential,
            }),
            None,
        ));
        String::new()
    } else {
        password
    };
    let structured = lines
        .map(|line| {
            if line.trim_start().starts_with("otpauth://") {
                return (
                    StructuredPassLine::Otp(OtpFieldTemplate::BareUrl),
                    Some(line.trim().to_string()),
                );
            }

            let Some((raw_key, raw_value)) = line.split_once(':') else {
                return (StructuredPassLine::Preserved(line.to_string()), None);
            };

            let title = raw_key.trim().to_string();
            if title.is_empty() {
                return (StructuredPassLine::Preserved(line.to_string()), None);
            }

            if is_username_field_key(&title) {
                return (
                    StructuredPassLine::Username(UsernameFieldTemplate {
                        raw_key: raw_key.to_string(),
                        separator_spacing: leading_spacing(raw_value),
                    }),
                    Some(raw_value.trim().to_string()),
                );
            }

            if is_otpauth_line(&title, raw_value, line) {
                return (
                    StructuredPassLine::Otp(OtpFieldTemplate::Field {
                        raw_key: raw_key.to_string(),
                        separator_spacing: leading_spacing(raw_value),
                    }),
                    Some(trim_leading_spacing(raw_value)),
                );
            }

            (
                StructuredPassLine::Field(DynamicFieldTemplate {
                    raw_key: raw_key.to_string(),
                    title,
                    separator_spacing: leading_spacing(raw_value),
                    sensitive: is_sensitive_field(raw_key),
                }),
                Some(trim_leading_spacing(raw_value)),
            )
        })
        .collect::<Vec<_>>();
    primary.extend(structured);
    (password, primary)
}

fn leading_spacing(value: &str) -> String {
    value
        .chars()
        .take_while(char::is_ascii_whitespace)
        .collect()
}

fn trim_leading_spacing(value: &str) -> String {
    value
        .trim_start_matches(|c: char| c.is_ascii_whitespace())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{pass_file_has_otp, searchable_pass_fields, SearchablePassField};

    fn field(key: &str, value: &str) -> SearchablePassField {
        SearchablePassField {
            key: key.to_string(),
            value: value.to_string(),
            normalized_value: value.to_lowercase(),
        }
    }

    #[test]
    fn username_aliases_share_the_username_key() {
        assert_eq!(
            searchable_pass_fields("secret\nlogin: Alice\nuser: Bob\nusername: Carol"),
            vec![
                field("username", "Alice"),
                field("username", "Bob"),
                field("username", "Carol"),
            ]
        );
    }

    #[test]
    fn dynamic_fields_are_indexed_by_their_pass_file_keys() {
        assert_eq!(
            searchable_pass_fields("secret\nUrl: https://example.com\nemail: Person@Example.com"),
            vec![
                field("url", "https://example.com"),
                field("email", "Person@Example.com"),
            ]
        );
    }

    #[test]
    fn otp_lines_are_not_indexed_for_search() {
        assert_eq!(
            searchable_pass_fields("secret\notpauth://totp/Example\notpauth: otpauth://totp/Alt"),
            Vec::<SearchablePassField>::new()
        );
    }

    #[test]
    fn pass_file_otp_detection_tracks_structured_or_bare_urls() {
        assert!(pass_file_has_otp(
            "secret\notpauth://totp/Example?secret=ABC"
        ));
        assert!(pass_file_has_otp(
            "secret\notpauth: otpauth://totp/Example?secret=ABC"
        ));
        assert!(!pass_file_has_otp(
            "secret\nusername: alice\nurl: https://example.com"
        ));
    }

    #[test]
    fn password_lines_and_preserved_text_do_not_become_search_fields() {
        assert_eq!(
            searchable_pass_fields(
                "secret-value\nnotes without colon\n  \nurl: https://example.com"
            ),
            vec![field("url", "https://example.com")]
        );
    }

    #[test]
    fn old_json_fields_are_ordinary_fields() {
        assert_eq!(
            searchable_pass_fields("secret\npasskey: ordinary value"),
            vec![field("passkey", "ordinary value")]
        );
    }
}

/// Native binary credentials have no password fields to export.
pub fn export_entry_text(bytes: &[u8]) -> Result<Option<String>, crate::PasswordEntryError> {
    #[cfg(feature = "passless")]
    if keycord_passkey::inspect_passless(bytes).is_some() {
        return Ok(None);
    }
    entry_text(bytes).map(Some)
}
