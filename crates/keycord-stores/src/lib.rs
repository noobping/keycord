//! Password-store repositories and management for Keycord.

pub mod entry_files;
pub mod error;
pub mod host;
pub mod integrated;
pub mod integrated_recipients;
pub mod labels;
pub mod management;
pub mod path_validation;
pub mod paths;
pub mod recipient_page;
pub mod recipients;
#[cfg(feature = "ui")]
pub mod ui;

pub use error::StoreRecipientsError;
pub use recipients::{
    relevant_store_recipient_scopes, StoreRecipients, StoreRecipientsPrivateKeyRequirement,
    ROOT_STORE_RECIPIENTS_SCOPE,
};
