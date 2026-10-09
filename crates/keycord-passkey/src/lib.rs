//! Optional Android Password Store and Passless storage, and credential exchange.

#[cfg(any(feature = "passkey", feature = "passless"))]
mod common;
#[cfg(feature = "passkey")]
mod credential;
#[cfg(any(feature = "passkey", feature = "passless"))]
mod cxf;
#[cfg(any(feature = "passkey", feature = "passless"))]
mod import;
mod mime;
#[cfg(feature = "passless")]
mod passless;

#[cfg(any(feature = "passkey", feature = "passless"))]
pub use common::is_passkey_entry_label;
#[cfg(feature = "passkey")]
pub use credential::{
    build_passkey_storage_entry, decode_passkey_storage_value, encode_passkey_storage_value,
    inspect_passkey_storage_value, validate_passkey_entry_label, PasskeyCredential,
    PasskeyStorageEntry, RelyingParty, UserInfo,
};
#[cfg(any(feature = "passkey", feature = "passless"))]
pub use cxf::parse_cxf_passkey_json;
#[cfg(any(feature = "passkey", feature = "passless"))]
pub use import::{ImportedCredential, PasskeyFormat, PreparedPasskey};
#[cfg(feature = "passkey")]
pub fn import_cxf_passkey_json(input: &str) -> Result<PasskeyCredential, String> {
    parse_cxf_passkey_json(input)?.android()
}
pub use mime::{PASSKEY_MIME_PACKAGE, PASSKEY_MIME_TYPES};
#[cfg(feature = "passless")]
pub use passless::{decode_passless, inspect_passless, PasslessCredential};

#[cfg(any(feature = "passkey", feature = "passless"))]
pub mod request;
#[cfg(all(feature = "ui", any(feature = "passkey", feature = "passless")))]
pub mod ui;
