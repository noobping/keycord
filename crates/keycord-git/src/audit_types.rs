//! Audit data shared by enabled and disabled Git feature profiles.

pub const STORE_GIT_AUDIT_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditCatalog {
    pub stores: Vec<StoreGitAuditStore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditStore {
    pub store_root: String,
    pub default_branch: Option<String>,
    pub branches: Vec<StoreGitAuditBranchRef>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditBranchRef {
    pub full_ref: String,
    pub name: String,
    pub remote: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditCommitPage {
    pub commits: Vec<StoreGitAuditCommit>,
    pub has_more: bool,
    pub next_page: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditCommit {
    pub oid: String,
    pub short_oid: String,
    pub subject: String,
    pub author: String,
    pub authored_at: String,
    pub committer: String,
    pub committed_at: String,
    pub message: String,
    pub changed_paths: Vec<StoreGitAuditPathChange>,
    pub verification: StoreGitAuditVerification,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditPathChange {
    pub status: String,
    pub path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditVerificationState {
    Verified,
    Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditVerificationMode {
    BranchTipRecipients,
    CommitHistoryRecipients,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditVerificationMethod {
    KeycordOpenPgp,
    HostGitGpg,
    HostGitSsh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreGitAuditUnverifiedReason {
    NoSignature,
    MalformedSignature,
    InvalidSignature,
    SigningKeyUnavailable,
    SignerNotAuthorized,
    NoResolvableStandardRecipients,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoreGitAuditVerification {
    pub state: StoreGitAuditVerificationState,
    pub mode: StoreGitAuditVerificationMode,
    pub method: Option<StoreGitAuditVerificationMethod>,
    pub used_commit_history_fallback: bool,
    pub reason: Option<StoreGitAuditUnverifiedReason>,
    pub signer_fingerprint: Option<String>,
    pub signer_label: Option<String>,
}

pub fn audit_unverified_reason_message(reason: StoreGitAuditUnverifiedReason) -> &'static str {
    match reason {
        StoreGitAuditUnverifiedReason::NoSignature => "No signature",
        StoreGitAuditUnverifiedReason::MalformedSignature => "Malformed or unsupported signature",
        StoreGitAuditUnverifiedReason::InvalidSignature => "Cryptographically invalid signature",
        StoreGitAuditUnverifiedReason::SigningKeyUnavailable => {
            "Signing key not available in Keycord"
        }
        StoreGitAuditUnverifiedReason::SignerNotAuthorized => {
            "Signer not in the branch recipient set"
        }
        StoreGitAuditUnverifiedReason::NoResolvableStandardRecipients => {
            "No resolvable standard recipient keys"
        }
    }
}
