use super::audit_types::{
    StoreGitAuditCatalog, StoreGitAuditCommitPage, StoreGitAuditUnverifiedReason,
    StoreGitAuditVerificationMethod, StoreGitAuditVerificationMode, StoreGitAuditVerificationState,
};

pub fn discover_store_git_audit_catalog(
    _store_roots: &[String],
) -> Result<StoreGitAuditCatalog, String> {
    touch_disabled_audit_types();
    Err("Audit features are disabled in this build.".to_string())
}

pub fn load_store_git_audit_commit_page(
    _store_root: &str,
    _full_ref: &str,
    _use_commit_history_recipients: bool,
    _page: usize,
) -> Result<StoreGitAuditCommitPage, String> {
    touch_disabled_audit_types();
    Err("Audit features are disabled in this build.".to_string())
}

fn touch_disabled_audit_types() {
    let _ = StoreGitAuditVerificationState::Verified;
    let _ = StoreGitAuditVerificationState::Unverified;
    let _ = StoreGitAuditVerificationMode::BranchTipRecipients;
    let _ = StoreGitAuditVerificationMode::CommitHistoryRecipients;
    let _ = [
        StoreGitAuditVerificationMethod::KeycordOpenPgp,
        StoreGitAuditVerificationMethod::HostGitGpg,
        StoreGitAuditVerificationMethod::HostGitSsh,
    ];
    let _ = [
        StoreGitAuditUnverifiedReason::NoSignature,
        StoreGitAuditUnverifiedReason::MalformedSignature,
        StoreGitAuditUnverifiedReason::InvalidSignature,
        StoreGitAuditUnverifiedReason::SigningKeyUnavailable,
        StoreGitAuditUnverifiedReason::SignerNotAuthorized,
        StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients,
    ];
}
