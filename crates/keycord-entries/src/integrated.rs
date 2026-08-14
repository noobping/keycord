//! Integrated password-entry encryption, persistence, and Git orchestration.

use crate::{
    PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError,
    PasswordEntryWriteProgress,
};
use keycord_keys::{
    PrivateKeyReadinessError, RipassoCrypto, INCOMPATIBLE_PRIVATE_KEY_ERROR,
    LOCKED_PRIVATE_KEY_ERROR, MISSING_PRIVATE_KEY_ERROR,
};
use keycord_runtime::{log_error, secure_fs::write_atomic_file};
use keycord_stores::integrated::StoreRecipientCrypto;
use keycord_stores::StoreRecipientsPrivateKeyRequirement;
use ripasso::crypto::Crypto;
use ripasso::pass::{Comment, KeyRingStatus, OwnerTrustLevel, Recipient};
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER: &str = "keycord-require-all-private-keys-v1";

/// A recipient resolved by the Stores boundary into the owned data Entries
/// needs for encryption.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IntegratedEntryRecipient {
    pub name: String,
    pub fingerprint: String,
    pub fingerprint_bytes: [u8; 20],
}

/// Supplies generic managed-key operations to integrated entry crypto.
pub trait IntegratedEntryKeyPort: Send + Sync {
    fn ensure_private_key_is_ready(
        &self,
        fingerprint: &str,
    ) -> Result<(), PrivateKeyReadinessError>;

    fn load_crypto(&self, fingerprint: &str) -> Result<RipassoCrypto, String>;

    fn decrypt_with_hardware_private_key(
        &self,
        fingerprint: &str,
        ciphertext: &[u8],
    ) -> Result<Option<String>, String>;

    fn fingerprint_from_string(&self, fingerprint: &str) -> Result<[u8; 20], String>;
}

/// Supplies store paths and recipient policy without giving Entries ownership
/// of store configuration.
pub trait IntegratedEntryStorePort: Send + Sync {
    fn entry_file_path(&self, store_root: &str, label: &str) -> Result<PathBuf, String>;
    fn existing_entry_file_path(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<Option<PathBuf>, String>;
    fn desired_entry_file_path(&self, store_root: &str, label: &str) -> Result<PathBuf, String>;
    fn cleanup_empty_store_dirs(&self, store_root: &str, entry_path: &Path) -> Result<(), String>;

    fn read_recipient_contents_for_label(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, String>;
    fn resolve_recipients_from_contents(
        &self,
        contents: &str,
    ) -> Result<Vec<IntegratedEntryRecipient>, String>;
    fn encryption_context_fingerprint_from_contents(
        &self,
        contents: &str,
    ) -> Result<String, String>;
    fn private_key_requirement_from_contents(
        &self,
        contents: &str,
    ) -> StoreRecipientsPrivateKeyRequirement;
    fn effective_private_key_requirement(
        &self,
        configured: StoreRecipientsPrivateKeyRequirement,
        recipient_count: usize,
    ) -> StoreRecipientsPrivateKeyRequirement;

    fn private_key_requirement_for_label(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<StoreRecipientsPrivateKeyRequirement, String>;
    fn required_private_key_fingerprints_for_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<Vec<String>, String>;
    fn decryption_candidate_fingerprints_for_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<Vec<String>, String>;
    fn password_entry_is_readable(&self, store_root: &str, label: &str) -> bool;
}

/// Supplies repository-specific path and commit effects.
pub trait IntegratedEntryGitPort: Send + Sync {
    fn supports_host_command_features(&self) -> bool;

    fn password_entry_git_path(
        &self,
        store_root: &Path,
        entry_path: &Path,
    ) -> Result<String, String>;

    fn maybe_commit_git_paths(
        &self,
        store_root: &str,
        message: &str,
        paths: &[String],
        explicit_fingerprint: Option<&str>,
    );

    fn commit_private_key_requiring_unlock(
        &self,
        store_root: &str,
        explicit_fingerprint: &str,
    ) -> Result<Option<String>, String>;
}

/// The three acyclic subject ports needed by integrated entry operations.
#[derive(Clone, Copy)]
pub struct IntegratedEntryPorts<'a> {
    pub keys: &'a dyn IntegratedEntryKeyPort,
    pub stores: &'a dyn IntegratedEntryStorePort,
    pub git: &'a dyn IntegratedEntryGitPort,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum RequiredPrivateKeyRecipient {
    Standard { fingerprint: String },
}

/// An encryption context resolved for one recipient scope.
pub struct IntegratedCryptoContext<'a> {
    ports: IntegratedEntryPorts<'a>,
    crypto: RipassoCrypto,
    recipients: Vec<Recipient>,
    fingerprint: String,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    required_private_key_recipients: Vec<RequiredPrivateKeyRecipient>,
}

impl<'a> IntegratedCryptoContext<'a> {
    pub fn load_for_fingerprint(
        ports: IntegratedEntryPorts<'a>,
        fingerprint: &str,
    ) -> Result<Self, String> {
        let crypto = ports.keys.load_crypto(fingerprint)?;
        Ok(Self {
            ports,
            crypto,
            recipients: Vec::new(),
            fingerprint: fingerprint.to_string(),
            private_key_requirement: StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
            required_private_key_recipients: vec![RequiredPrivateKeyRecipient::Standard {
                fingerprint: fingerprint.to_string(),
            }],
        })
    }

    pub fn load_for_label(
        ports: IntegratedEntryPorts<'a>,
        store_root: &str,
        label: &str,
    ) -> Result<Self, String> {
        let contents = ports
            .stores
            .read_recipient_contents_for_label(store_root, label)?;
        Self::load_for_recipient_contents(ports, &contents)
    }

    pub fn fingerprint_for_label(
        ports: IntegratedEntryPorts<'a>,
        store_root: &str,
        label: &str,
    ) -> Result<String, String> {
        let contents = ports
            .stores
            .read_recipient_contents_for_label(store_root, label)?;
        Self::fingerprint_for_recipient_contents(ports, &contents)
    }

    pub fn load_for_recipient_contents(
        ports: IntegratedEntryPorts<'a>,
        contents: &str,
    ) -> Result<Self, String> {
        let resolved = ports.stores.resolve_recipients_from_contents(contents)?;
        let recipients = standard_recipients_from_resolved(&resolved);
        let fingerprint = ports
            .stores
            .encryption_context_fingerprint_from_contents(contents)?;
        let private_key_requirement = ports.stores.effective_private_key_requirement(
            ports.stores.private_key_requirement_from_contents(contents),
            recipients.len(),
        );
        let required_private_key_recipients = required_recipients_from_resolved(&resolved);
        let crypto = ports.keys.load_crypto(&fingerprint)?;
        Ok(Self {
            ports,
            crypto,
            recipients,
            fingerprint,
            private_key_requirement,
            required_private_key_recipients,
        })
    }

    pub fn fingerprint_for_recipient_contents(
        ports: IntegratedEntryPorts<'a>,
        contents: &str,
    ) -> Result<String, String> {
        ports
            .stores
            .encryption_context_fingerprint_from_contents(contents)
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn decrypt_entry(&self, entry_path: &Path) -> Result<String, String> {
        let ciphertext = read_entry_ciphertext(entry_path)?;
        match self.private_key_requirement {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey => self
                .decrypt_ciphertext_for_fingerprint(&self.fingerprint, &self.crypto, &ciphertext),
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys => self
                .decrypt_password_entry_requiring_all_private_keys(
                    &ciphertext,
                    &self.required_private_key_recipients,
                ),
        }
    }

    pub fn encrypt_contents_with_existing(
        &self,
        contents: &str,
        _existing_ciphertext: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        match self.private_key_requirement {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey => {
                encrypt_password_entry_with_crypto(&self.crypto, &self.recipients, contents)
            }
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys => self
                .encrypt_password_entry_requiring_all_private_keys(
                    contents,
                    &self.required_private_key_recipients,
                ),
        }
    }

    fn decrypt_ciphertext_for_fingerprint(
        &self,
        fingerprint: &str,
        crypto: &RipassoCrypto,
        ciphertext: &[u8],
    ) -> Result<String, String> {
        if let Some(secret) = self
            .ports
            .keys
            .decrypt_with_hardware_private_key(fingerprint, ciphertext)?
        {
            return Ok(secret);
        }

        crypto
            .decrypt_string(ciphertext)
            .map_err(|err| err.to_string())
    }

    fn decrypt_password_entry_requiring_all_private_keys(
        &self,
        ciphertext: &[u8],
        required_recipients: &[RequiredPrivateKeyRecipient],
    ) -> Result<String, String> {
        let mut current = ciphertext.to_vec();

        for (index, recipient) in required_recipients.iter().enumerate() {
            let decrypted = self.decrypt_required_private_key_layer(recipient, &current)?;
            if index + 1 == required_recipients.len() {
                return String::from_utf8(decrypted).map_err(|err| err.to_string());
            }
            current = unwrap_required_private_key_layer(&decrypted)?;
        }

        Err("No recipients were found for this password entry.".to_string())
    }

    fn decrypt_required_private_key_layer(
        &self,
        recipient: &RequiredPrivateKeyRecipient,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, String> {
        match recipient {
            RequiredPrivateKeyRecipient::Standard { fingerprint } => {
                let context = Self::load_for_fingerprint(self.ports, fingerprint)?;
                let decrypted = context.decrypt_ciphertext_for_fingerprint(
                    fingerprint,
                    &context.crypto,
                    ciphertext,
                )?;
                Ok(decrypted.into_bytes())
            }
        }
    }

    fn encrypt_password_entry_requiring_all_private_keys(
        &self,
        contents: &str,
        required_recipients: &[RequiredPrivateKeyRecipient],
    ) -> Result<Vec<u8>, String> {
        let Some((last_recipient, outer_recipients)) = required_recipients.split_last() else {
            return Err("No recipients were found for this password entry.".to_string());
        };

        let mut current =
            self.encrypt_for_required_private_key_recipient(last_recipient, contents.as_bytes())?;
        for recipient in outer_recipients.iter().rev() {
            let wrapped = wrap_required_private_key_layer(&current);
            current =
                self.encrypt_for_required_private_key_recipient(recipient, wrapped.as_bytes())?;
        }
        Ok(current)
    }

    fn encrypt_for_required_private_key_recipient(
        &self,
        recipient: &RequiredPrivateKeyRecipient,
        payload: &[u8],
    ) -> Result<Vec<u8>, String> {
        match recipient {
            RequiredPrivateKeyRecipient::Standard { fingerprint } => {
                let context = Self::load_for_fingerprint(self.ports, fingerprint)?;
                let text = String::from_utf8(payload.to_vec()).map_err(|err| err.to_string())?;
                let recipient = Recipient {
                    name: fingerprint.clone(),
                    comment: Comment {
                        pre_comment: None,
                        post_comment: None,
                    },
                    key_id: fingerprint.clone(),
                    fingerprint: Some(self.ports.keys.fingerprint_from_string(fingerprint)?),
                    key_ring_status: KeyRingStatus::InKeyRing,
                    trust_level: OwnerTrustLevel::Ultimate,
                    not_usable: false,
                };
                encrypt_password_entry_with_crypto(&context.crypto, &[recipient], &text)
            }
        }
    }
}

impl StoreRecipientCrypto for IntegratedCryptoContext<'_> {
    fn encrypt_contents_with_existing(
        &self,
        contents: &str,
        existing_ciphertext: Option<&[u8]>,
    ) -> Result<Vec<u8>, String> {
        Self::encrypt_contents_with_existing(self, contents, existing_ciphertext)
    }

    fn fingerprint(&self) -> &str {
        Self::fingerprint(self)
    }
}

/// Integrated entry operations bound to explicit subject ports.
#[derive(Clone, Copy)]
pub struct IntegratedEntryBackend<'a> {
    ports: IntegratedEntryPorts<'a>,
}

impl<'a> IntegratedEntryBackend<'a> {
    pub const fn new(ports: IntegratedEntryPorts<'a>) -> Self {
        Self { ports }
    }

    pub fn ports(self) -> IntegratedEntryPorts<'a> {
        self.ports
    }

    pub fn load_crypto_for_fingerprint(
        self,
        fingerprint: &str,
    ) -> Result<IntegratedCryptoContext<'a>, String> {
        IntegratedCryptoContext::load_for_fingerprint(self.ports, fingerprint)
    }

    pub fn load_crypto_for_recipient_contents(
        self,
        contents: &str,
    ) -> Result<IntegratedCryptoContext<'a>, String> {
        IntegratedCryptoContext::load_for_recipient_contents(self.ports, contents)
    }

    pub fn fingerprint_for_label(self, store_root: &str, label: &str) -> Result<String, String> {
        IntegratedCryptoContext::fingerprint_for_label(self.ports, store_root, label)
    }

    pub fn fingerprint_for_recipient_contents(self, contents: &str) -> Result<String, String> {
        IntegratedCryptoContext::fingerprint_for_recipient_contents(self.ports, contents)
    }

    pub fn required_private_key_fingerprints_for_entry(
        self,
        store_root: &str,
        label: &str,
    ) -> Result<Vec<String>, String> {
        self.ports
            .stores
            .required_private_key_fingerprints_for_entry(store_root, label)
    }

    pub fn read_password_entry(
        self,
        store_root: &str,
        label: &str,
    ) -> Result<String, PasswordEntryError> {
        self.read_password_entry_with_progress(store_root, label, &mut |_| {})
    }

    pub fn read_password_entry_with_progress(
        self,
        store_root: &str,
        label: &str,
        report_progress: &mut dyn FnMut(PasswordEntryReadProgress),
    ) -> Result<String, PasswordEntryError> {
        let entry_path = self
            .ports
            .stores
            .entry_file_path(store_root, label)
            .map_err(PasswordEntryError::other)?;
        if matches!(
            self.ports
                .stores
                .private_key_requirement_for_label(store_root, label),
            Ok(StoreRecipientsPrivateKeyRequirement::AllManagedKeys)
        ) {
            let required_fingerprints = self
                .ports
                .stores
                .required_private_key_fingerprints_for_entry(store_root, label)
                .map_err(|_| PasswordEntryError::missing_private_key(MISSING_PRIVATE_KEY_ERROR))?;
            self.ensure_required_private_keys_are_ready(&required_fingerprints)?;
            let context = IntegratedCryptoContext::load_for_label(self.ports, store_root, label)
                .map_err(password_entry_error_from_integrated_message)?;
            return context
                .decrypt_entry(&entry_path)
                .map_err(password_entry_error_from_integrated_message);
        }

        let mut saw_locked_key = false;
        let mut saw_incompatible_key = false;
        let mut last_error = None;
        let candidates = self
            .ports
            .stores
            .decryption_candidate_fingerprints_for_entry(store_root, label)
            .map_err(PasswordEntryError::other)?;
        let _ = report_progress;
        for fingerprint in candidates {
            match self.decrypt_entry_for_fingerprint(&fingerprint, &entry_path) {
                Ok(secret) => return Ok(secret),
                Err(err) => match password_entry_error_from_integrated_message(err) {
                    PasswordEntryError::LockedPrivateKey(message) => {
                        saw_locked_key = true;
                        last_error = Some(PasswordEntryError::LockedPrivateKey(message));
                    }
                    PasswordEntryError::IncompatiblePrivateKey(message) => {
                        saw_incompatible_key = true;
                        last_error = Some(PasswordEntryError::IncompatiblePrivateKey(message));
                    }
                    other => last_error = Some(other),
                },
            }
        }

        if saw_locked_key {
            return Err(PasswordEntryError::locked_private_key(
                LOCKED_PRIVATE_KEY_ERROR,
            ));
        }
        if saw_incompatible_key {
            return Err(PasswordEntryError::incompatible_private_key(
                INCOMPATIBLE_PRIVATE_KEY_ERROR,
            ));
        }
        Err(last_error
            .unwrap_or_else(|| PasswordEntryError::missing_private_key(MISSING_PRIVATE_KEY_ERROR)))
    }

    pub fn read_password_line(
        self,
        store_root: &str,
        label: &str,
    ) -> Result<String, PasswordEntryError> {
        let secret = self.read_password_entry(store_root, label)?;
        Ok(secret.lines().next().unwrap_or_default().to_string())
    }

    pub fn password_entry_is_readable(self, store_root: &str, label: &str) -> bool {
        self.ports
            .stores
            .password_entry_is_readable(store_root, label)
    }

    pub fn save_password_entry(
        self,
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError> {
        self.save_password_entry_with_progress(store_root, label, contents, overwrite, &mut |_| {})
    }

    pub fn save_password_entry_with_progress(
        self,
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
        report_progress: &mut dyn FnMut(PasswordEntryWriteProgress),
    ) -> Result<(), PasswordEntryWriteError> {
        let existing_entry_path = self
            .ports
            .stores
            .existing_entry_file_path(store_root, label)
            .map_err(password_entry_write_error_from_integrated_message)?;
        let entry_path = self
            .ports
            .stores
            .desired_entry_file_path(store_root, label)
            .map_err(password_entry_write_error_from_integrated_message)?;
        let git_message = if existing_entry_path.is_some() {
            format!("Update password for {label}")
        } else {
            format!("Add password for {label}")
        };
        if existing_entry_path.is_some() && !overwrite {
            return Err(PasswordEntryWriteError::already_exists(
                "That password entry already exists.",
            ));
        }

        let context = IntegratedCryptoContext::load_for_label(self.ports, store_root, label)
            .map_err(password_entry_write_error_from_integrated_message)?;
        let previous_ciphertext = existing_entry_path
            .as_ref()
            .map(fs::read)
            .transpose()
            .map_err(|err| password_entry_write_error_from_integrated_message(err.to_string()))?;
        let _ = report_progress;
        let ciphertext = context
            .encrypt_contents_with_existing(contents, previous_ciphertext.as_deref())
            .map_err(password_entry_write_error_from_integrated_message)?;
        let existing_git_path = existing_entry_path
            .as_ref()
            .filter(|existing_path| **existing_path != entry_path)
            .map(|existing_path| {
                self.ports
                    .git
                    .password_entry_git_path(Path::new(store_root), existing_path)
            })
            .transpose()
            .map_err(password_entry_write_error_from_integrated_message)?;
        let new_git_path = self
            .ports
            .git
            .password_entry_git_path(Path::new(store_root), &entry_path)
            .map_err(password_entry_write_error_from_integrated_message)?;
        let result = write_entry_ciphertext(&entry_path, &ciphertext)
            .and_then(|()| {
                if let Some(existing_path) = existing_entry_path
                    .as_ref()
                    .filter(|existing_path| **existing_path != entry_path)
                {
                    fs::remove_file(existing_path).map_err(|err| err.to_string())?;
                }
                Ok(())
            })
            .map_err(password_entry_write_error_from_integrated_message);
        if result.is_ok() {
            let paths = existing_git_path
                .into_iter()
                .chain(std::iter::once(new_git_path))
                .collect::<Vec<_>>();
            self.ports.git.maybe_commit_git_paths(
                store_root,
                &git_message,
                &paths,
                Some(context.fingerprint()),
            );
        }
        result
    }

    pub fn rename_password_entry(
        self,
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), PasswordEntryWriteError> {
        let commit_fingerprint = self.commit_identity_fingerprint_for_label(store_root, old_label);
        let old_path = self
            .ports
            .stores
            .existing_entry_file_path(store_root, old_label)
            .map_err(password_entry_write_error_from_integrated_message)?
            .ok_or_else(|| {
                PasswordEntryWriteError::entry_not_found(format!(
                    "Password entry '{old_label}' was not found."
                ))
            })?;
        if self
            .ports
            .stores
            .existing_entry_file_path(store_root, new_label)
            .map_err(password_entry_write_error_from_integrated_message)?
            .is_some()
        {
            return Err(PasswordEntryWriteError::already_exists(
                "That password entry already exists.",
            ));
        }
        let new_path = self
            .ports
            .stores
            .desired_entry_file_path(store_root, new_label)
            .map_err(password_entry_write_error_from_integrated_message)?;

        ensure_parent_dir(&new_path).map_err(password_entry_write_error_from_integrated_message)?;
        fs::rename(&old_path, &new_path).map_err(|err| password_entry_write_error_from_io(&err))?;
        let old_git_path = self
            .ports
            .git
            .password_entry_git_path(Path::new(store_root), &old_path)
            .map_err(password_entry_write_error_from_integrated_message)?;
        let new_git_path = self
            .ports
            .git
            .password_entry_git_path(Path::new(store_root), &new_path)
            .map_err(password_entry_write_error_from_integrated_message)?;
        let result = self
            .ports
            .stores
            .cleanup_empty_store_dirs(store_root, &old_path)
            .map_err(password_entry_write_error_from_integrated_message);
        if result.is_ok() {
            self.ports.git.maybe_commit_git_paths(
                store_root,
                &format!("Rename password from {old_label} to {new_label}"),
                &[old_git_path, new_git_path],
                commit_fingerprint.as_deref(),
            );
        }
        result
    }

    pub fn delete_password_entry(
        self,
        store_root: &str,
        label: &str,
    ) -> Result<(), PasswordEntryWriteError> {
        let commit_fingerprint = self.commit_identity_fingerprint_for_label(store_root, label);
        let entry_path = self
            .ports
            .stores
            .existing_entry_file_path(store_root, label)
            .map_err(password_entry_write_error_from_integrated_message)?
            .ok_or_else(|| {
                PasswordEntryWriteError::entry_not_found(format!(
                    "Password entry '{label}' was not found."
                ))
            })?;
        let git_path = self
            .ports
            .git
            .password_entry_git_path(Path::new(store_root), &entry_path)
            .map_err(password_entry_write_error_from_integrated_message)?;
        fs::remove_file(&entry_path).map_err(|err| password_entry_write_error_from_io(&err))?;
        let result = self
            .ports
            .stores
            .cleanup_empty_store_dirs(store_root, &entry_path)
            .map_err(password_entry_write_error_from_integrated_message);
        if result.is_ok() {
            self.ports.git.maybe_commit_git_paths(
                store_root,
                &format!("Remove password for {label}"),
                &[git_path],
                commit_fingerprint.as_deref(),
            );
        }
        result
    }

    pub fn git_commit_private_key_requiring_unlock_for_entry(
        self,
        store_root: &str,
        label: &str,
    ) -> Result<Option<String>, String> {
        if !self.ports.git.supports_host_command_features() {
            return Ok(None);
        }
        let fingerprint = self.fingerprint_for_label(store_root, label)?;
        self.ports
            .git
            .commit_private_key_requiring_unlock(store_root, &fingerprint)
    }

    fn commit_identity_fingerprint_for_label(
        self,
        store_root: &str,
        label: &str,
    ) -> Option<String> {
        if !self.password_entry_is_readable(store_root, label) {
            return None;
        }
        match self.fingerprint_for_label(store_root, label) {
            Ok(fingerprint) => Some(fingerprint),
            Err(err) => {
                log_error(format!(
                    "Failed to resolve integrated Git commit identity for {store_root}/{label}: {err}"
                ));
                None
            }
        }
    }

    fn ensure_required_private_keys_are_ready(
        self,
        fingerprints: &[String],
    ) -> Result<(), PasswordEntryError> {
        for fingerprint in fingerprints {
            self.ports
                .keys
                .ensure_private_key_is_ready(fingerprint)
                .map_err(password_entry_error_from_private_key_readiness)?;
        }
        Ok(())
    }

    fn decrypt_entry_for_fingerprint(
        self,
        fingerprint: &str,
        entry_path: &Path,
    ) -> Result<String, String> {
        let ciphertext = read_entry_ciphertext(entry_path)?;
        self.ports
            .keys
            .ensure_private_key_is_ready(fingerprint)
            .map_err(private_key_readiness_error_to_string)?;
        let context = IntegratedCryptoContext::load_for_fingerprint(self.ports, fingerprint)?;
        context.decrypt_ciphertext_for_fingerprint(fingerprint, &context.crypto, &ciphertext)
    }
}

fn standard_recipients_from_resolved(resolved: &[IntegratedEntryRecipient]) -> Vec<Recipient> {
    resolved
        .iter()
        .map(|recipient| Recipient {
            name: recipient.name.clone(),
            comment: Comment {
                pre_comment: None,
                post_comment: None,
            },
            key_id: recipient.fingerprint.clone(),
            fingerprint: Some(recipient.fingerprint_bytes),
            key_ring_status: KeyRingStatus::InKeyRing,
            trust_level: OwnerTrustLevel::Ultimate,
            not_usable: false,
        })
        .collect()
}

fn required_recipients_from_resolved(
    resolved: &[IntegratedEntryRecipient],
) -> Vec<RequiredPrivateKeyRecipient> {
    resolved
        .iter()
        .map(|recipient| RequiredPrivateKeyRecipient::Standard {
            fingerprint: recipient.fingerprint.clone(),
        })
        .collect()
}

fn read_entry_ciphertext(entry_path: &Path) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(entry_path).map_err(|err| err.to_string())?;
    if metadata.len() == 0 {
        return Err("empty password file".to_string());
    }
    fs::read(entry_path).map_err(|err| err.to_string())
}

fn wrap_required_private_key_layer(ciphertext: &[u8]) -> String {
    format!(
        "{REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER}\n{}",
        encode_hex(ciphertext)
    )
}

fn unwrap_required_private_key_layer(payload: &[u8]) -> Result<Vec<u8>, String> {
    let payload = std::str::from_utf8(payload).map_err(|err| err.to_string())?;
    let (header, body) = payload
        .split_once('\n')
        .ok_or_else(|| "Invalid all-keys encrypted password entry.".to_string())?;
    if header.trim() != REQUIRE_ALL_PRIVATE_KEYS_LAYER_HEADER {
        return Err("Invalid all-keys encrypted password entry.".to_string());
    }
    decode_hex(body.trim())
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(encoded, "{byte:02x}").expect("writing hex into a string should not fail");
    }
    encoded
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if !value.len().is_multiple_of(2) {
        return Err("Invalid all-keys encrypted password entry.".to_string());
    }
    let mut decoded = Vec::with_capacity(value.len() / 2);
    let mut index = 0;
    while index < value.len() {
        let byte = u8::from_str_radix(&value[index..index + 2], 16)
            .map_err(|_| "Invalid all-keys encrypted password entry.".to_string())?;
        decoded.push(byte);
        index += 2;
    }
    Ok(decoded)
}

fn encrypt_password_entry_with_crypto(
    crypto: &RipassoCrypto,
    recipients: &[Recipient],
    contents: &str,
) -> Result<Vec<u8>, String> {
    if recipients.is_empty() {
        return Err("No recipients were found for this password entry.".to_string());
    }
    crypto
        .encrypt_string(contents, recipients)
        .map_err(|err| err.to_string())
}

fn password_entry_error_from_private_key_readiness(
    error: PrivateKeyReadinessError,
) -> PasswordEntryError {
    match error {
        PrivateKeyReadinessError::Missing(message) => {
            PasswordEntryError::missing_private_key(message)
        }
        PrivateKeyReadinessError::Locked(message) => {
            PasswordEntryError::locked_private_key(message)
        }
        PrivateKeyReadinessError::Incompatible(message) => {
            PasswordEntryError::incompatible_private_key(message)
        }
        PrivateKeyReadinessError::Other(message) => PasswordEntryError::other(message),
    }
}

fn private_key_readiness_error_to_string(error: PrivateKeyReadinessError) -> String {
    match error {
        PrivateKeyReadinessError::Missing(message)
        | PrivateKeyReadinessError::Locked(message)
        | PrivateKeyReadinessError::Incompatible(message)
        | PrivateKeyReadinessError::Other(message) => message,
    }
}

fn password_entry_error_from_integrated_message(message: impl Into<String>) -> PasswordEntryError {
    let message = message.into();
    match message.as_str() {
        MISSING_PRIVATE_KEY_ERROR => PasswordEntryError::missing_private_key(message),
        LOCKED_PRIVATE_KEY_ERROR => PasswordEntryError::locked_private_key(message),
        INCOMPATIBLE_PRIVATE_KEY_ERROR => PasswordEntryError::incompatible_private_key(message),
        _ => PasswordEntryError::other(message),
    }
}

fn password_entry_write_error_from_integrated_message(
    message: impl Into<String>,
) -> PasswordEntryWriteError {
    let message = message.into();
    match message.as_str() {
        MISSING_PRIVATE_KEY_ERROR => PasswordEntryWriteError::MissingPrivateKey(message),
        LOCKED_PRIVATE_KEY_ERROR => PasswordEntryWriteError::LockedPrivateKey(message),
        INCOMPATIBLE_PRIVATE_KEY_ERROR => PasswordEntryWriteError::IncompatiblePrivateKey(message),
        _ => PasswordEntryWriteError::other(message),
    }
}

fn password_entry_write_error_from_io(err: &io::Error) -> PasswordEntryWriteError {
    match err.kind() {
        io::ErrorKind::AlreadyExists => PasswordEntryWriteError::already_exists(err.to_string()),
        io::ErrorKind::NotFound => PasswordEntryWriteError::entry_not_found(err.to_string()),
        _ => password_entry_write_error_from_integrated_message(err.to_string()),
    }
}

fn write_entry_ciphertext(entry_path: &Path, ciphertext: &[u8]) -> Result<(), String> {
    write_atomic_file(entry_path, ciphertext).map_err(|err| err.to_string())
}

fn ensure_parent_dir(entry_path: &Path) -> Result<(), String> {
    if let Some(parent) = entry_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        decode_hex, password_entry_error_from_integrated_message,
        password_entry_write_error_from_integrated_message, password_entry_write_error_from_io,
        unwrap_required_private_key_layer, wrap_required_private_key_layer,
    };
    use crate::{PasswordEntryError, PasswordEntryWriteError};
    use keycord_keys::{
        INCOMPATIBLE_PRIVATE_KEY_ERROR, LOCKED_PRIVATE_KEY_ERROR, MISSING_PRIVATE_KEY_ERROR,
    };
    use std::io;

    #[test]
    fn all_keys_layer_round_trips_binary_ciphertext() {
        let ciphertext = [0, 1, 127, 128, 254, 255];
        let wrapped = wrap_required_private_key_layer(&ciphertext);
        assert_eq!(
            unwrap_required_private_key_layer(wrapped.as_bytes()).expect("unwrap layer"),
            ciphertext
        );
        assert!(decode_hex("abc").is_err());
        assert!(decode_hex("xx").is_err());
    }

    #[test]
    fn integrated_read_errors_map_to_public_variants_and_toasts() {
        let missing = password_entry_error_from_integrated_message(MISSING_PRIVATE_KEY_ERROR);
        assert!(matches!(missing, PasswordEntryError::MissingPrivateKey(_)));
        assert_eq!(
            missing.toast_message(),
            Some("Add a private key in Preferences.")
        );

        let locked = password_entry_error_from_integrated_message(LOCKED_PRIVATE_KEY_ERROR);
        assert!(matches!(locked, PasswordEntryError::LockedPrivateKey(_)));
        assert_eq!(locked.toast_message(), None);

        let incompatible =
            password_entry_error_from_integrated_message(INCOMPATIBLE_PRIVATE_KEY_ERROR);
        assert!(matches!(
            incompatible,
            PasswordEntryError::IncompatiblePrivateKey(_)
        ));
        assert_eq!(
            incompatible.toast_message(),
            Some("This key can't open your items.")
        );
    }

    #[test]
    fn integrated_write_errors_map_to_public_variants_and_toasts() {
        let missing = password_entry_write_error_from_integrated_message(MISSING_PRIVATE_KEY_ERROR);
        assert!(matches!(
            missing,
            PasswordEntryWriteError::MissingPrivateKey(_)
        ));
        assert_eq!(
            missing.save_toast_message(),
            "Add a private key in Preferences."
        );

        let locked = password_entry_write_error_from_integrated_message(LOCKED_PRIVATE_KEY_ERROR);
        assert!(matches!(
            locked,
            PasswordEntryWriteError::LockedPrivateKey(_)
        ));
        assert_eq!(
            locked.save_toast_message(),
            "Unlock the key in Preferences."
        );

        let incompatible =
            password_entry_write_error_from_integrated_message(INCOMPATIBLE_PRIVATE_KEY_ERROR);
        assert!(matches!(
            incompatible,
            PasswordEntryWriteError::IncompatiblePrivateKey(_)
        ));
        assert_eq!(
            incompatible.save_toast_message(),
            "This key can't open your items."
        );
    }

    #[test]
    fn integrated_write_io_errors_classify_by_io_kind() {
        assert!(matches!(
            password_entry_write_error_from_io(&io::Error::from(io::ErrorKind::AlreadyExists)),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
        assert!(matches!(
            password_entry_write_error_from_io(&io::Error::from(io::ErrorKind::NotFound)),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
    }
}
