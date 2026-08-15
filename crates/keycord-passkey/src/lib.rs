//! Passkey credentials and local credential-exchange workflows.

mod credential;
mod mime;

#[cfg(feature = "passkey")]
pub use credential::{
    build_passkey_storage_entry, encode_passkey_storage_value, export_cxf_passkey_json,
    generate_passkey_credential, import_cxf_passkey_json,
};
pub use credential::{
    decode_passkey_storage_value, PasskeyCredential, PasskeyRegistrationState, PasskeyStorageEntry,
    PASSKEY_FIELD_KEY,
};
pub use mime::{PASSKEY_MIME_PACKAGE, PASSKEY_MIME_TYPES};

#[cfg(feature = "passkey")]
pub mod request;

#[cfg(feature = "ui")]
pub mod ui;
