//! Root composition for the extracted integrated Entries backend.

use super::git::{
    git_commit_private_key_requiring_unlock_for_fingerprint, maybe_commit_git_paths,
    password_entry_git_path,
};
use crate::composition::backend::{
    PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError,
    StoreRecipientsPrivateKeyRequirement,
};
use keycord_entries::integrated::{
    IntegratedCryptoContext, IntegratedEntryBackend, IntegratedEntryGitPort,
    IntegratedEntryKeyPort, IntegratedEntryPorts, IntegratedEntryRecipient,
    IntegratedEntryStorePort,
};
use keycord_keys::{
    borrow_unlocked_hardware_private_key, build_ripasso_crypto_from_key_ring,
    decrypt_with_hardware_session, fingerprint_from_string, load_available_standard_key_ring,
    load_ripasso_key_ring,
};
use keycord_keys::{PrivateKeyReadinessError, RipassoCrypto};
use keycord_stores::{integrated_recipients, paths};
use std::path::{Path, PathBuf};

struct RootIntegratedEntryKeyPort;
struct RootIntegratedEntryStorePort;
struct RootIntegratedEntryGitPort;

static ROOT_INTEGRATED_ENTRY_KEY_PORT: RootIntegratedEntryKeyPort = RootIntegratedEntryKeyPort;
static ROOT_INTEGRATED_ENTRY_STORE_PORT: RootIntegratedEntryStorePort =
    RootIntegratedEntryStorePort;
static ROOT_INTEGRATED_ENTRY_GIT_PORT: RootIntegratedEntryGitPort = RootIntegratedEntryGitPort;

impl IntegratedEntryKeyPort for RootIntegratedEntryKeyPort {
    fn ensure_private_key_is_ready(
        &self,
        fingerprint: &str,
    ) -> Result<(), PrivateKeyReadinessError> {
        keycord_keys::ensure_ripasso_private_key_is_ready(fingerprint)
    }

    fn load_crypto(&self, fingerprint: &str) -> Result<RipassoCrypto, String> {
        let key_ring = load_ripasso_key_ring(fingerprint)?;
        build_ripasso_crypto_from_key_ring(fingerprint, key_ring)
    }

    fn decrypt_with_hardware_private_key(
        &self,
        fingerprint: &str,
        ciphertext: &[u8],
    ) -> Result<Option<String>, String> {
        let Some(session) = borrow_unlocked_hardware_private_key(fingerprint)? else {
            return Ok(None);
        };
        decrypt_with_hardware_session(&session, ciphertext)
            .map(Some)
            .map_err(|err| err.to_string())
    }

    fn fingerprint_from_string(&self, fingerprint: &str) -> Result<[u8; 20], String> {
        fingerprint_from_string(fingerprint)
    }
}

impl IntegratedEntryStorePort for RootIntegratedEntryStorePort {
    fn entry_file_path(&self, store_root: &str, label: &str) -> Result<PathBuf, String> {
        paths::entry_file_path(store_root, label)
    }

    fn existing_entry_file_path(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<Option<PathBuf>, String> {
        paths::existing_entry_file_path(store_root, label)
    }

    fn desired_entry_file_path(&self, store_root: &str, label: &str) -> Result<PathBuf, String> {
        paths::desired_entry_file_path(store_root, label)
    }

    fn cleanup_empty_store_dirs(&self, store_root: &str, entry_path: &Path) -> Result<(), String> {
        paths::cleanup_empty_store_dirs(store_root, entry_path)
    }

    fn read_recipient_contents_for_label(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, String> {
        let recipients_file = paths::recipients_file_for_label(store_root, label)?;
        integrated_recipients::read_store_recipient_file_contents(&recipients_file)
    }

    fn resolve_recipients_from_contents(
        &self,
        contents: &str,
    ) -> Result<Vec<IntegratedEntryRecipient>, String> {
        let key_ring = load_available_standard_key_ring()?;
        integrated_recipients::resolved_recipients_from_contents(contents, &key_ring)?
            .into_iter()
            .map(|recipient| match recipient {
                integrated_recipients::ResolvedRecipient::Standard {
                    fingerprint,
                    cert,
                    requested_id,
                } => {
                    let name = cert
                        .userids()
                        .map(|user_id| user_id.userid().to_string())
                        .find(|value| !value.trim().is_empty())
                        .unwrap_or(requested_id);
                    Ok(IntegratedEntryRecipient {
                        name,
                        fingerprint: cert.fingerprint().to_hex(),
                        fingerprint_bytes: fingerprint,
                    })
                }
            })
            .collect()
    }

    fn encryption_context_fingerprint_from_contents(
        &self,
        contents: &str,
    ) -> Result<String, String> {
        let key_ring = load_available_standard_key_ring()?;
        integrated_recipients::encryption_context_fingerprint_from_contents(contents, &key_ring)
    }

    fn private_key_requirement_from_contents(
        &self,
        contents: &str,
    ) -> StoreRecipientsPrivateKeyRequirement {
        integrated_recipients::private_key_requirement_from_contents(contents)
    }

    fn effective_private_key_requirement(
        &self,
        configured: StoreRecipientsPrivateKeyRequirement,
        recipient_count: usize,
    ) -> StoreRecipientsPrivateKeyRequirement {
        integrated_recipients::effective_private_key_requirement(configured, recipient_count)
    }

    fn private_key_requirement_for_label(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<StoreRecipientsPrivateKeyRequirement, String> {
        integrated_recipients::private_key_requirement_for_label(store_root, label)
    }

    fn required_private_key_fingerprints_for_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<Vec<String>, String> {
        integrated_recipients::required_private_key_fingerprints_for_entry(store_root, label)
    }

    fn decryption_candidate_fingerprints_for_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<Vec<String>, String> {
        integrated_recipients::decryption_candidate_fingerprints_for_entry(store_root, label)
    }

    fn password_entry_is_readable(&self, store_root: &str, label: &str) -> bool {
        integrated_recipients::password_entry_is_readable(store_root, label)
    }
}

impl IntegratedEntryGitPort for RootIntegratedEntryGitPort {
    fn supports_host_command_features(&self) -> bool {
        keycord_runtime::capabilities::supports_host_command_features()
    }

    fn password_entry_git_path(
        &self,
        store_root: &Path,
        entry_path: &Path,
    ) -> Result<String, String> {
        password_entry_git_path(store_root, entry_path)
    }

    fn maybe_commit_git_paths(
        &self,
        store_root: &str,
        message: &str,
        paths: &[String],
        explicit_fingerprint: Option<&str>,
    ) {
        maybe_commit_git_paths(
            store_root,
            message,
            paths.iter().cloned(),
            explicit_fingerprint,
        );
    }

    fn commit_private_key_requiring_unlock(
        &self,
        store_root: &str,
        explicit_fingerprint: &str,
    ) -> Result<Option<String>, String> {
        git_commit_private_key_requiring_unlock_for_fingerprint(store_root, explicit_fingerprint)
    }
}

pub(super) const fn integrated_entry_ports() -> IntegratedEntryPorts<'static> {
    IntegratedEntryPorts {
        keys: &ROOT_INTEGRATED_ENTRY_KEY_PORT,
        stores: &ROOT_INTEGRATED_ENTRY_STORE_PORT,
        git: &ROOT_INTEGRATED_ENTRY_GIT_PORT,
    }
}

pub(super) const fn integrated_entry_backend() -> IntegratedEntryBackend<'static> {
    IntegratedEntryBackend::new(integrated_entry_ports())
}

pub fn read_password_entry(store_root: &str, label: &str) -> Result<String, PasswordEntryError> {
    integrated_entry_backend().read_password_entry(store_root, label)
}

pub fn read_password_entry_with_progress(
    store_root: &str,
    label: &str,
    report_progress: &mut dyn FnMut(PasswordEntryReadProgress),
) -> Result<String, PasswordEntryError> {
    integrated_entry_backend().read_password_entry_with_progress(store_root, label, report_progress)
}

pub fn read_password_line(store_root: &str, label: &str) -> Result<String, PasswordEntryError> {
    integrated_entry_backend().read_password_line(store_root, label)
}

pub fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    integrated_entry_backend().password_entry_is_readable(store_root, label)
}

pub fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    integrated_entry_backend().save_password_entry(store_root, label, contents, overwrite)
}

pub fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), PasswordEntryWriteError> {
    integrated_entry_backend().rename_password_entry(store_root, old_label, new_label)
}

pub fn delete_password_entry(store_root: &str, label: &str) -> Result<(), PasswordEntryWriteError> {
    integrated_entry_backend().delete_password_entry(store_root, label)
}

pub fn git_commit_private_key_requiring_unlock_for_entry(
    store_root: &str,
    label: &str,
) -> Result<Option<String>, String> {
    integrated_entry_backend().git_commit_private_key_requiring_unlock_for_entry(store_root, label)
}

#[cfg(test)]
pub fn required_private_key_fingerprints_for_entry(
    store_root: &str,
    label: &str,
) -> Result<Vec<String>, String> {
    integrated_entry_backend().required_private_key_fingerprints_for_entry(store_root, label)
}

pub(super) type RootIntegratedCryptoContext = IntegratedCryptoContext<'static>;
