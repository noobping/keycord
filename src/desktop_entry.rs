pub const PASSKEY_MIME_TYPES: &str =
    "application/vnd.keycord.passkey-request+json;application/vnd.keycord.passkey+json;";

pub fn passkey_fields(passkey_enabled: bool) -> (&'static str, String) {
    if passkey_enabled {
        (" %f", format!("MimeType={PASSKEY_MIME_TYPES}\n"))
    } else {
        ("", String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::{passkey_fields, PASSKEY_MIME_TYPES};

    #[test]
    fn passkey_builds_advertise_request_handlers() {
        let (open_argument, mime_types) = passkey_fields(true);

        assert_eq!(open_argument, " %f");
        assert_eq!(mime_types, format!("MimeType={PASSKEY_MIME_TYPES}\n"));
    }

    #[test]
    fn builds_without_passkeys_do_not_advertise_request_handlers() {
        let (open_argument, mime_types) = passkey_fields(false);

        assert!(open_argument.is_empty());
        assert!(mime_types.is_empty());
    }
}
