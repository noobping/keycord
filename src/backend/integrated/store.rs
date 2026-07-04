use super::crypto::IntegratedCryptoContext;
use super::entries::read_password_entry;
use super::git::{maybe_commit_git_paths, password_entry_git_path};
use super::keys::store_recipients_error_from_integrated_message;
use super::paths::{
    collect_password_entry_files, desired_entry_file_path, ensure_store_directory,
    label_from_entry_path, recipients_file_for_label, recipients_file_for_relative_dir,
    with_updated_recipient_file,
};
use super::recipients::{
    preferred_ripasso_private_key_fingerprint_for_entry, standard_recipient_file_contents,
};
use crate::backend::{
    PasswordEntryError, StoreRecipients, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement,
};
use crate::support::git::{ensure_store_git_repository, has_git_repository};
use crate::support::secure_fs::write_atomic_file;
use std::fs;
use std::path::{Path, PathBuf};

fn decrypted_store_entries(
    store_dir: &Path,
    store_root: &str,
    scoped_recipients_path: &Path,
) -> Result<Vec<(PathBuf, String)>, String> {
    let mut decrypted = Vec::new();
    let entry_paths =
        collect_root_scoped_entry_paths(store_dir, store_root, scoped_recipients_path)?;

    for entry_path in entry_paths {
        let label = label_from_entry_path(store_dir, &entry_path)?;
        let secret = read_password_entry(store_root, &label).map_err(|err| err.to_string())?;
        decrypted.push((entry_path, secret));
    }

    Ok(decrypted)
}

fn collect_root_scoped_entry_paths(
    store_dir: &Path,
    store_root: &str,
    scoped_recipients_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut scoped_paths = Vec::new();
    for entry_path in collect_password_entry_files(store_dir)? {
        let label = label_from_entry_path(store_dir, &entry_path)?;
        if recipients_file_for_label(store_root, &label)? != scoped_recipients_path {
            continue;
        }
        scoped_paths.push(entry_path);
    }

    Ok(scoped_paths)
}

pub(in crate::backend) fn try_initialize_empty_store_recipients(
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
    let should_initialize_git = !has_git_repository(store_root);

    with_updated_recipient_file(&recipients_path, &recipients_contents, || Ok(()))?;

    if should_initialize_git {
        ensure_store_git_repository(store_root)?;
    }

    maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        std::iter::once(password_entry_git_path(&store_dir, &recipients_path)?),
        None,
    );

    Ok(true)
}

pub fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    store_recipients_private_key_requiring_unlock_for_relative_dir(store_root, ".")
}

pub fn store_recipients_private_key_requiring_unlock_for_relative_dir(
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
            read_password_entry(store_root, &label),
            Err(PasswordEntryError::LockedPrivateKey(_))
        ) {
            continue;
        }

        return preferred_ripasso_private_key_fingerprint_for_entry(store_root, &label).map(Some);
    }

    Ok(None)
}

pub fn save_store_recipients(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    save_store_recipients_for_relative_dir(store_root, ".", recipients, private_key_requirement)
}

pub fn save_store_recipients_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let store_dir = ensure_store_directory(store_root)
        .map_err(store_recipients_error_from_integrated_message)?;
    let recipients_path = recipients_file_for_relative_dir(store_root, relative_dir)
        .map_err(store_recipients_error_from_integrated_message)?;
    let decrypted_entries = decrypted_store_entries(&store_dir, store_root, &recipients_path)
        .map_err(store_recipients_error_from_integrated_message)?;
    let recipients_contents =
        standard_recipient_file_contents(recipients.standard(), private_key_requirement);
    let context = IntegratedCryptoContext::load_for_recipient_contents(&recipients_contents)
        .map_err(store_recipients_error_from_integrated_message)?;
    let should_initialize_git = !recipients_path.exists() && !has_git_repository(store_root);
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
            committed_entry_paths.push(password_entry_git_path(&store_dir, &updated_entry_path)?);
            if updated_entry_path != *entry_path {
                committed_entry_paths.push(password_entry_git_path(&store_dir, entry_path)?);
            }
        }
        Ok(())
    })
    .map_err(store_recipients_error_from_integrated_message)?;

    if should_initialize_git {
        ensure_store_git_repository(store_root)
            .map_err(store_recipients_error_from_integrated_message)?;
    }

    maybe_commit_git_paths(
        store_root,
        "Update password store recipients",
        std::iter::once(
            password_entry_git_path(&store_dir, &recipients_path)
                .map_err(store_recipients_error_from_integrated_message)?,
        )
        .chain(committed_entry_paths),
        Some(context.fingerprint()),
    );

    Ok(())
}
