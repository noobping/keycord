//! Integrated-backend store operations.
//!
//! The store crate owns the filesystem transaction and recipient scoping. Crypto and Git are
//! supplied by the application shell so this crate does not depend on either implementation.

use std::fs;
use std::path::{Path, PathBuf};

use keycord_runtime::secure_fs::write_atomic_file;
use thiserror::Error;

use crate::error::store_recipients_error_from_integrated_message;
use crate::integrated_recipients::{
    preferred_ripasso_private_key_fingerprint_for_entry, standard_recipient_file_contents,
};
use crate::paths::{
    collect_password_entry_files, desired_entry_file_path, ensure_store_directory,
    label_from_entry_path, recipients_file_for_label, recipients_file_for_relative_dir,
    with_updated_recipient_file,
};
use crate::{StoreRecipients, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement};

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum StoreEntryReadError {
    #[error("{0}")]
    Locked(String),
    #[error("{0}")]
    Other(String),
}

pub trait StoreRecipientCrypto {
    fn encrypt_contents_with_existing(
        &self,
        contents: &str,
        existing_ciphertext: Option<&[u8]>,
    ) -> Result<Vec<u8>, String>;

    fn fingerprint(&self) -> &str;
}

/// Backend capabilities needed by integrated store-recipient operations.
///
/// Git paths are opaque strings here. Their interpretation belongs to the Git adapter.
pub trait IntegratedStorePorts {
    type Crypto: StoreRecipientCrypto;

    fn read_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, StoreEntryReadError>;

    fn load_crypto(&self, recipients_contents: &str) -> Result<Self::Crypto, String>;

    fn has_git_repository(&self, store_root: &str) -> bool;

    fn ensure_git_repository(&self, store_root: &str) -> Result<(), String>;

    fn git_path(&self, store_root: &Path, path: &Path) -> Result<String, String>;

    fn maybe_commit_git_paths(
        &self,
        store_root: &str,
        message: &str,
        paths: Vec<String>,
        explicit_fingerprint: Option<&str>,
    );
}

fn collect_root_scoped_entry_paths(
    store_dir: &Path,
    store_root: &str,
    scoped_recipients_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut scoped_paths = Vec::new();
    for entry_path in collect_password_entry_files(store_dir)? {
        let label = label_from_entry_path(store_dir, &entry_path)?;
        if recipients_file_for_label(store_root, &label)? == scoped_recipients_path {
            scoped_paths.push(entry_path);
        }
    }
    Ok(scoped_paths)
}

fn decrypted_store_entries<P: IntegratedStorePorts + ?Sized>(
    ports: &P,
    store_dir: &Path,
    store_root: &str,
    scoped_recipients_path: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let mut decrypted = Vec::new();
    for entry_path in
        collect_root_scoped_entry_paths(store_dir, store_root, scoped_recipients_path)?
    {
        let label = label_from_entry_path(store_dir, &entry_path)?;
        let secret = ports
            .read_password_entry(store_root, &label)
            .map_err(|err| err.to_string())?;
        decrypted.push((entry_path, secret));
    }
    Ok(decrypted)
}

pub fn try_initialize_empty_store_recipients_with<P: IntegratedStorePorts + ?Sized>(
    ports: &P,
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<bool, String> {
    let store_dir = ensure_store_directory(store_root)?;
    let recipients_path = recipients_file_for_relative_dir(store_root, ".")?;
    if recipients_path.exists() || !collect_password_entry_files(&store_dir)?.is_empty() {
        return Ok(false);
    }

    let recipients_contents =
        standard_recipient_file_contents(recipients.standard(), private_key_requirement);
    let should_initialize_git = !ports.has_git_repository(store_root);

    with_updated_recipient_file(&recipients_path, &recipients_contents, || Ok(()))?;

    if should_initialize_git {
        ports.ensure_git_repository(store_root)?;
    }

    let recipients_git_path = ports.git_path(&store_dir, &recipients_path)?;
    ports.maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        vec![recipients_git_path],
        None,
    );

    Ok(true)
}

pub fn store_recipients_private_key_requiring_unlock_with<P: IntegratedStorePorts + ?Sized>(
    ports: &P,
    store_root: &str,
) -> Result<Option<String>, String> {
    store_recipients_private_key_requiring_unlock_for_relative_dir_with(ports, store_root, ".")
}

pub fn store_recipients_private_key_requiring_unlock_for_relative_dir_with<
    P: IntegratedStorePorts + ?Sized,
>(
    ports: &P,
    store_root: &str,
    relative_dir: &str,
) -> Result<Option<String>, String> {
    let store_dir = ensure_store_directory(store_root)?;
    let scoped_recipients_path = recipients_file_for_relative_dir(store_root, relative_dir)?;

    for entry_path in collect_password_entry_files(&store_dir)? {
        let label = label_from_entry_path(&store_dir, &entry_path)?;
        if recipients_file_for_label(store_root, &label)? != scoped_recipients_path {
            continue;
        }
        if !matches!(
            ports.read_password_entry(store_root, &label),
            Err(StoreEntryReadError::Locked(_))
        ) {
            continue;
        }

        return preferred_ripasso_private_key_fingerprint_for_entry(store_root, &label).map(Some);
    }

    Ok(None)
}

pub fn save_store_recipients_with<P: IntegratedStorePorts + ?Sized>(
    ports: &P,
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    save_store_recipients_for_relative_dir_with(
        ports,
        store_root,
        ".",
        recipients,
        private_key_requirement,
    )
}

pub fn save_store_recipients_for_relative_dir_with<P: IntegratedStorePorts + ?Sized>(
    ports: &P,
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let map_error = store_recipients_error_from_integrated_message;
    let store_dir = ensure_store_directory(store_root).map_err(map_error)?;
    let recipients_path =
        recipients_file_for_relative_dir(store_root, relative_dir).map_err(map_error)?;
    let decrypted_entries =
        decrypted_store_entries(ports, &store_dir, store_root, &recipients_path)
            .map_err(map_error)?;
    let recipients_contents =
        standard_recipient_file_contents(recipients.standard(), private_key_requirement);
    let context = ports.load_crypto(&recipients_contents).map_err(map_error)?;
    let should_initialize_git = !recipients_path.exists() && !ports.has_git_repository(store_root);
    let mut committed_entry_paths = Vec::new();

    with_updated_recipient_file(&recipients_path, &recipients_contents, || {
        for (entry_path, secret) in &decrypted_entries {
            let label = label_from_entry_path(&store_dir, entry_path)?;
            let updated_entry_path = desired_entry_file_path(store_root, &label)?;
            let ciphertext = context.encrypt_contents_with_existing(secret, None)?;
            write_atomic_file(&updated_entry_path, &ciphertext).map_err(|err| err.to_string())?;
            if updated_entry_path != *entry_path {
                fs::remove_file(entry_path).map_err(|err| err.to_string())?;
            }
            committed_entry_paths.push(ports.git_path(&store_dir, &updated_entry_path)?);
            if updated_entry_path != *entry_path {
                committed_entry_paths.push(ports.git_path(&store_dir, entry_path)?);
            }
        }
        Ok(())
    })
    .map_err(map_error)?;

    if should_initialize_git {
        ports.ensure_git_repository(store_root).map_err(map_error)?;
    }

    let recipients_git_path = ports
        .git_path(&store_dir, &recipients_path)
        .map_err(map_error)?;
    let mut paths = Vec::with_capacity(committed_entry_paths.len() + 1);
    paths.push(recipients_git_path);
    paths.extend(committed_entry_paths);
    ports.maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        paths,
        Some(context.fingerprint()),
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        save_store_recipients_for_relative_dir_with, try_initialize_empty_store_recipients_with,
        IntegratedStorePorts, StoreEntryReadError, StoreRecipientCrypto,
    };
    use crate::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};

    struct TestCrypto;

    impl StoreRecipientCrypto for TestCrypto {
        fn encrypt_contents_with_existing(
            &self,
            contents: &str,
            _existing_ciphertext: Option<&[u8]>,
        ) -> Result<Vec<u8>, String> {
            if contents == "fail" {
                Err("test encryption failure".to_string())
            } else {
                Ok(format!("encrypted:{contents}").into_bytes())
            }
        }

        fn fingerprint(&self) -> &str {
            "test-fingerprint"
        }
    }

    #[derive(Default)]
    struct TestPorts {
        entries: HashMap<String, String>,
        reads: RefCell<Vec<String>>,
        initialized: RefCell<Vec<String>>,
        commits: RefCell<Vec<(Vec<String>, Option<String>)>>,
    }

    impl IntegratedStorePorts for TestPorts {
        type Crypto = TestCrypto;

        fn read_password_entry(
            &self,
            _store_root: &str,
            label: &str,
        ) -> Result<String, StoreEntryReadError> {
            self.reads.borrow_mut().push(label.to_string());
            self.entries
                .get(label)
                .cloned()
                .ok_or_else(|| StoreEntryReadError::Other("missing test entry".to_string()))
        }

        fn load_crypto(&self, _recipients_contents: &str) -> Result<Self::Crypto, String> {
            Ok(TestCrypto)
        }

        fn has_git_repository(&self, store_root: &str) -> bool {
            Path::new(store_root).join(".git").is_dir()
        }

        fn ensure_git_repository(&self, store_root: &str) -> Result<(), String> {
            self.initialized.borrow_mut().push(store_root.to_string());
            fs::create_dir_all(Path::new(store_root).join(".git")).map_err(|err| err.to_string())
        }

        fn git_path(&self, store_root: &Path, path: &Path) -> Result<String, String> {
            path.strip_prefix(store_root)
                .map(|relative| relative.to_string_lossy().to_string())
                .map_err(|err| err.to_string())
        }

        fn maybe_commit_git_paths(
            &self,
            _store_root: &str,
            _message: &str,
            paths: Vec<String>,
            explicit_fingerprint: Option<&str>,
        ) {
            self.commits
                .borrow_mut()
                .push((paths, explicit_fingerprint.map(ToString::to_string)));
        }
    }

    fn temp_store(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{nanos}"))
    }

    #[test]
    fn empty_store_initialization_writes_recipients_and_initializes_git() {
        let store = temp_store("keycord-store-init");
        let ports = TestPorts::default();
        let recipients = StoreRecipients::new(vec!["alice@example.com".to_string()]);

        assert!(try_initialize_empty_store_recipients_with(
            &ports,
            store.to_string_lossy().as_ref(),
            &recipients,
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("initialize store"));

        assert_eq!(
            fs::read_to_string(store.join(".gpg-id")).expect("read recipients"),
            "alice@example.com\n"
        );
        assert_eq!(ports.initialized.borrow().len(), 1);
        assert_eq!(ports.commits.borrow()[0].0, vec![".gpg-id"]);

        fs::remove_dir_all(store).expect("remove store");
    }

    #[test]
    fn nested_update_reencrypts_only_entries_in_that_recipient_scope() {
        let store = temp_store("keycord-store-scope");
        fs::create_dir_all(store.join("team")).expect("create store");
        fs::write(store.join(".gpg-id"), "root@example.com\n").expect("write root recipients");
        fs::write(store.join("team/.gpg-id"), "old@example.com\n")
            .expect("write nested recipients");
        fs::write(store.join("root.gpg"), "old-root").expect("write root entry");
        fs::write(store.join("team/service.gpg"), "old-team").expect("write nested entry");

        let ports = TestPorts {
            entries: HashMap::from([
                ("root".to_string(), "root-secret".to_string()),
                ("team/service".to_string(), "team-secret".to_string()),
            ]),
            ..TestPorts::default()
        };
        let recipients = StoreRecipients::new(vec!["new@example.com".to_string()]);

        save_store_recipients_for_relative_dir_with(
            &ports,
            store.to_string_lossy().as_ref(),
            "team",
            &recipients,
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save nested recipients");

        assert_eq!(ports.reads.borrow().as_slice(), ["team/service"]);
        assert_eq!(
            fs::read(store.join("team/service.gpg")).expect("read nested entry"),
            b"encrypted:team-secret"
        );
        assert_eq!(
            fs::read(store.join("root.gpg")).expect("read root entry"),
            b"old-root"
        );
        assert_eq!(
            fs::read_to_string(store.join("team/.gpg-id")).expect("read nested recipients"),
            "new@example.com\n"
        );
        assert_eq!(
            ports.commits.borrow()[0],
            (
                vec!["team/.gpg-id".to_string(), "team/service.gpg".to_string()],
                Some("test-fingerprint".to_string())
            )
        );

        fs::remove_dir_all(store).expect("remove store");
    }

    #[test]
    fn failed_reencryption_restores_the_previous_recipient_file() {
        let store = temp_store("keycord-store-rollback");
        fs::create_dir_all(&store).expect("create store");
        fs::write(store.join(".gpg-id"), "old@example.com\n").expect("write recipients");
        fs::write(store.join("service.gpg"), "old-ciphertext").expect("write entry");
        let ports = TestPorts {
            entries: HashMap::from([("service".to_string(), "fail".to_string())]),
            ..TestPorts::default()
        };
        let recipients = StoreRecipients::new(vec!["new@example.com".to_string()]);

        let error = save_store_recipients_for_relative_dir_with(
            &ports,
            store.to_string_lossy().as_ref(),
            ".",
            &recipients,
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect_err("encryption should fail");

        assert!(error.to_string().contains("test encryption failure"));
        assert_eq!(
            fs::read_to_string(store.join(".gpg-id")).expect("read recipients"),
            "old@example.com\n"
        );
        assert_eq!(
            fs::read(store.join("service.gpg")).expect("read entry"),
            b"old-ciphertext"
        );
        assert!(ports.commits.borrow().is_empty());

        fs::remove_dir_all(store).expect("remove store");
    }
}
