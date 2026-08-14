//! Cross-subject adapters supplied by the application binary.

pub mod about;
pub(crate) mod backend;
pub mod entries_ui;
pub mod git_audit;
pub mod git_signing;
pub mod git_ui;
pub mod host_access;
pub mod keys_sync;
pub mod keys_unlock;
pub mod localization;
pub mod navigation;
#[cfg(feature = "passkey")]
pub mod passkey_dialog;
pub mod preferences_ui;
pub mod stores_ui;
