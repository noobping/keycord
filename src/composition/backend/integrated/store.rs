use std::path::Path;

use keycord_stores::integrated::{self as store_core, IntegratedStorePorts, StoreEntryReadError};

use super::entries::{integrated_entry_backend, read_password_entry, RootIntegratedCryptoContext};
use super::git::{maybe_commit_git_paths, password_entry_git_path};
use crate::composition::backend::{
    PasswordEntryError, StoreRecipients, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement,
};
use keycord_git::{ensure_store_git_repository, has_git_repository};

struct RootIntegratedStorePorts;

impl IntegratedStorePorts for RootIntegratedStorePorts {
    type Crypto = RootIntegratedCryptoContext;

    fn read_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, StoreEntryReadError> {
        read_password_entry(store_root, label).map_err(|err| match err {
            PasswordEntryError::LockedPrivateKey(message) => StoreEntryReadError::Locked(message),
            other => StoreEntryReadError::Other(other.to_string()),
        })
    }

    fn load_crypto(&self, recipients_contents: &str) -> Result<Self::Crypto, String> {
        integrated_entry_backend().load_crypto_for_recipient_contents(recipients_contents)
    }

    fn has_git_repository(&self, store_root: &str) -> bool {
        has_git_repository(store_root)
    }

    fn ensure_git_repository(&self, store_root: &str) -> Result<(), String> {
        ensure_store_git_repository(store_root)
    }

    fn git_path(&self, store_root: &Path, path: &Path) -> Result<String, String> {
        password_entry_git_path(store_root, path)
    }

    fn maybe_commit_git_paths(
        &self,
        store_root: &str,
        message: &str,
        paths: Vec<String>,
        explicit_fingerprint: Option<&str>,
    ) {
        maybe_commit_git_paths(store_root, message, paths, explicit_fingerprint);
    }
}

pub(in crate::composition::backend) fn try_initialize_empty_store_recipients(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<bool, String> {
    store_core::try_initialize_empty_store_recipients_with(
        &RootIntegratedStorePorts,
        store_root,
        recipients,
        private_key_requirement,
    )
}

pub fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    store_core::store_recipients_private_key_requiring_unlock_with(
        &RootIntegratedStorePorts,
        store_root,
    )
}

pub fn store_recipients_private_key_requiring_unlock_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
) -> Result<Option<String>, String> {
    store_core::store_recipients_private_key_requiring_unlock_for_relative_dir_with(
        &RootIntegratedStorePorts,
        store_root,
        relative_dir,
    )
}

pub fn save_store_recipients(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    store_core::save_store_recipients_with(
        &RootIntegratedStorePorts,
        store_root,
        recipients,
        private_key_requirement,
    )
}

pub fn save_store_recipients_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    store_core::save_store_recipients_for_relative_dir_with(
        &RootIntegratedStorePorts,
        store_root,
        relative_dir,
        recipients,
        private_key_requirement,
    )
}
