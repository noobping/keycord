//! Git repository integration for Keycord.

pub const fn audit_available() -> bool {
    cfg!(all(target_os = "linux", feature = "audit"))
}

#[cfg(test)]
mod capability_tests {
    #[test]
    fn audit_availability_matches_the_platform_and_feature() {
        assert_eq!(
            super::audit_available(),
            cfg!(all(target_os = "linux", feature = "audit"))
        );
    }
}

#[cfg(feature = "audit")]
mod audit;
#[cfg(not(feature = "audit"))]
#[path = "audit_disabled.rs"]
mod audit;
mod audit_types;
mod command;
mod integrated;
pub mod operations;
mod remotes;
mod repository;
mod status;
mod sync;
mod types;
#[cfg(feature = "ui")]
pub mod ui;

pub use audit::discover_store_git_audit_catalog;
#[cfg(not(feature = "audit"))]
pub use audit::load_store_git_audit_commit_page;
#[cfg(feature = "audit")]
pub use audit::{load_store_git_audit_commit_page_with_ports, StoreGitAuditPorts};
pub use audit_types::{
    audit_unverified_reason_message, StoreGitAuditBranchRef, StoreGitAuditCatalog,
    StoreGitAuditCommit, StoreGitAuditCommitPage, StoreGitAuditPathChange, StoreGitAuditStore,
    StoreGitAuditUnverifiedReason, StoreGitAuditVerification, StoreGitAuditVerificationMethod,
    StoreGitAuditVerificationMode, StoreGitAuditVerificationState, STORE_GIT_AUDIT_PAGE_SIZE,
};
pub use integrated::{
    git_commit_private_key_requiring_unlock, maybe_commit_git_paths, password_entry_git_path,
    GitPrivateKey, IntegratedGitPorts,
};
pub use remotes::{
    add_store_git_remote, list_store_git_remotes, remove_store_git_remote, rename_store_git_remote,
    set_store_git_remote_url,
};
pub use repository::{
    ensure_store_git_repository, git_command_available, has_git_repository,
    password_store_git_state_summary,
};
pub use status::store_git_repository_status;
pub use sync::sync_store_repository;
pub use types::{GitRemote, StoreGitHead, StoreGitRepositoryStatus};

#[cfg(test)]
mod tests;
