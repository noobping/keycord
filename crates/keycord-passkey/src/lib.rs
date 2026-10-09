//! Optional Android Password Store credentials and local credential-exchange workflows.

#[cfg(feature = "passkey")]
mod credential;
#[cfg(feature = "passkey")]
mod cxf;
mod mime;

#[cfg(feature = "passkey")]
pub use credential::{
    build_passkey_storage_entry, decode_passkey_storage_value, encode_passkey_storage_value,
    inspect_passkey_storage_value, is_passkey_entry_label, validate_passkey_entry_label,
    PasskeyCredential, PasskeyStorageEntry, RelyingParty, UserInfo,
};
#[cfg(feature = "passkey")]
pub use cxf::import_cxf_passkey_json;
pub use mime::{PASSKEY_MIME_PACKAGE, PASSKEY_MIME_TYPES};

#[cfg(feature = "passkey")]
pub mod request;

#[cfg(feature = "ui")]
pub mod ui;
