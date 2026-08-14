//! Connects Git audit history to the configured key and recipient providers.

#[cfg(feature = "audit")]
use crate::composition::backend::{
    available_host_gpg_public_certs, available_standard_public_certs,
};
#[cfg(feature = "audit")]
use keycord_stores::recipients::{normalize_standard_recipient, parse_standard_recipients};

#[cfg(not(feature = "audit"))]
pub fn load_store_git_audit_commit_page(
    store_root: &str,
    full_ref: &str,
    use_commit_history_recipients: bool,
    page: usize,
) -> Result<keycord_git::StoreGitAuditCommitPage, String> {
    keycord_git::load_store_git_audit_commit_page(
        store_root,
        full_ref,
        use_commit_history_recipients,
        page,
    )
}

#[cfg(feature = "audit")]
pub fn load_store_git_audit_commit_page(
    store_root: &str,
    full_ref: &str,
    use_commit_history_recipients: bool,
    page: usize,
) -> Result<keycord_git::StoreGitAuditCommitPage, String> {
    keycord_git::load_store_git_audit_commit_page_with_ports(
        store_root,
        full_ref,
        use_commit_history_recipients,
        page,
        keycord_git::StoreGitAuditPorts {
            available_standard_public_certs,
            available_host_public_certs: available_host_gpg_public_certs,
            parse_standard_recipients,
            normalize_standard_recipient,
            standard_recipient_matches_user_id:
                keycord_stores::integrated_recipients::standard_recipient_matches_user_id,
            host_key_sync_enabled: crate::composition::keys_sync::private_key_sync_enabled,
        },
    )
}
