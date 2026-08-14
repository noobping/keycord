use crate::composition::backend::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
use keycord_git::ui::integrated_git_ports;
use keycord_runtime::capabilities::supports_host_command_features;
use keycord_stores::integrated_recipients::standard_recipient_file_contents;
use std::path::Path;

pub(super) fn git_commit_private_key_requiring_unlock_for_fingerprint(
    store_root: &str,
    fingerprint: &str,
) -> Result<Option<String>, String> {
    keycord_git::git_commit_private_key_requiring_unlock(
        store_root,
        Some(fingerprint),
        integrated_git_ports(),
    )
}

pub fn git_commit_private_key_requiring_unlock_for_store_recipients(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<Option<String>, String> {
    if !supports_host_command_features() {
        return Ok(None);
    }
    let standard_contents =
        standard_recipient_file_contents(recipients.standard(), private_key_requirement);
    let fingerprint = match super::entries::integrated_entry_backend()
        .fingerprint_for_recipient_contents(&standard_contents)
    {
        Ok(fingerprint) => fingerprint,
        Err(err)
            if err.contains("is not available in the app.")
                || err.contains("No recipients were found") =>
        {
            return Ok(None);
        }
        Err(err) => return Err(err),
    };

    keycord_git::git_commit_private_key_requiring_unlock(
        store_root,
        Some(&fingerprint),
        integrated_git_ports(),
    )
}

pub(super) fn password_entry_git_path(
    store_root: &Path,
    entry_path: &Path,
) -> Result<String, String> {
    keycord_git::password_entry_git_path(store_root, entry_path)
}

pub(super) fn maybe_commit_git_paths(
    store_root: &str,
    message: &str,
    paths: impl IntoIterator<Item = String>,
    explicit_fingerprint: Option<&str>,
) {
    keycord_git::maybe_commit_git_paths(
        store_root,
        message,
        paths,
        explicit_fingerprint,
        integrated_git_ports(),
    );
}
