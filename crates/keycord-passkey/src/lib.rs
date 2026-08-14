//! Passkey credentials and local credential-exchange workflows.

mod credential;

#[cfg(feature = "passkey")]
pub use credential::{
    build_passkey_storage_entry, encode_passkey_storage_value, export_cxf_passkey_json,
    generate_passkey_credential, import_cxf_passkey_json,
};
pub use credential::{
    decode_passkey_storage_value, PasskeyCredential, PasskeyRegistrationState, PasskeyStorageEntry,
    PASSKEY_FIELD_KEY,
};

#[cfg(feature = "passkey")]
pub mod request;

#[cfg(feature = "ui")]
pub mod ui;

pub const PASSKEY_MIME_PACKAGE: &str =
    include_str!("../data/io.github.noobping.keycord-passkey.xml");
pub const PASSKEY_MIME_TYPES: &str =
    "application/vnd.keycord.passkey-request+json;application/vnd.keycord.passkey+json;";
