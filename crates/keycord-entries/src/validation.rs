//! Validation rules for password-entry contents.

use keycord_runtime::validation::is_valid_email_address;

/// Validates every `email:` field after the password line.
pub fn validate_pass_file_email_fields(contents: &str) -> Result<(), &'static str> {
    for line in contents.lines().skip(1) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("email") {
            continue;
        }
        if !is_valid_email_address(value) {
            return Err("Email fields must use a valid email address.");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_pass_file_email_fields;

    #[test]
    fn pass_files_reject_invalid_email_fields() {
        assert_eq!(
            validate_pass_file_email_fields("secret\nemail: person@example.com"),
            Ok(())
        );
        assert_eq!(
            validate_pass_file_email_fields("secret\nEmail: invalid"),
            Err("Email fields must use a valid email address.")
        );
        assert_eq!(
            validate_pass_file_email_fields("secret\nnotes without separator"),
            Ok(())
        );
    }
}
