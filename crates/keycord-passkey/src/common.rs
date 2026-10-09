use std::path::Path;

pub(crate) const MAX_STORAGE_BYTES: usize = 256 * 1024;

pub(crate) fn key_error(_: openssl::error::ErrorStack) -> String {
    "The passkey contains invalid private key material.".into()
}

pub(crate) fn validate_rp_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 253
        || !value.is_ascii()
        || !value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
        })
    {
        return Err("Enter a valid passkey RP ID.".into());
    }
    Ok(())
}

/// Android's discoverable filenames. Other entry renames need no extra decryption.
pub fn is_passkey_entry_label(label: &str) -> bool {
    let path = Path::new(label.strip_suffix(".gpg").unwrap_or(label));
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.len() == 64 && name.bytes().all(|byte| byte.is_ascii_hexdigit()))
        && path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .is_some_and(|parent| validate_rp_id(parent).is_ok())
}
