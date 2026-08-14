//! Subject-neutral text validation shared by UI and domain crates.

use regex::Regex;
use std::sync::OnceLock;

fn email_regex() -> &'static Regex {
    static EMAIL_REGEX: OnceLock<Regex> = OnceLock::new();
    EMAIL_REGEX.get_or_init(|| {
        Regex::new(
            r"(?i)^[a-z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?)+$",
        )
        .expect("email validation regex should compile")
    })
}

pub fn is_valid_email_address(email: &str) -> bool {
    let email = email.trim();
    !email.is_empty() && email_regex().is_match(email)
}

pub fn validate_email_address(email: &str) -> Result<String, &'static str> {
    let email = email.trim();
    if email.is_empty() {
        return Err("Enter an email address.");
    }
    if !is_valid_email_address(email) {
        return Err("Enter a valid email address.");
    }

    Ok(email.to_string())
}

#[cfg(test)]
mod tests {
    use super::{is_valid_email_address, validate_email_address};

    #[test]
    fn email_addresses_require_a_domain_and_local_part() {
        assert!(is_valid_email_address("person@example.com"));
        assert!(is_valid_email_address("PERSON+tag@sub.example.com"));
        assert!(!is_valid_email_address("person"));
        assert!(!is_valid_email_address("person@localhost"));
        assert!(!is_valid_email_address("person@"));
    }

    #[test]
    fn email_validation_trims_input() {
        assert_eq!(
            validate_email_address("  person@example.com  "),
            Ok("person@example.com".to_string())
        );
        assert_eq!(validate_email_address(""), Err("Enter an email address."));
        assert_eq!(
            validate_email_address("invalid"),
            Err("Enter a valid email address.")
        );
    }
}
