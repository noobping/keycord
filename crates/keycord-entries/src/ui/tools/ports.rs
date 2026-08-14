//! Composition ports for entry-oriented tools.

use std::rc::Rc;
use std::sync::Arc;

use adw::ToastOverlay;
use keycord_shell::navigation::WindowChromeCallback;

use crate::clipboard::PromptEntryUnlock;
use crate::model::CollectItemsOptions;
use crate::tools::EntryRequest;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryToolKeySummary {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
}

#[derive(Clone)]
pub struct EntryToolPreferencesPorts {
    pub store_roots: Rc<dyn Fn() -> Vec<String>>,
    pub uses_integrated_backend: Rc<dyn Fn() -> bool>,
}

pub type CollectToolEntries = Arc<dyn Fn(CollectItemsOptions) -> Vec<EntryRequest> + Send + Sync>;
pub type ReadToolEntry = Arc<dyn Fn(String, String) -> Result<String, String> + Send + Sync>;

#[derive(Clone)]
pub struct EntryToolBackendPorts {
    pub collect_entries: CollectToolEntries,
    pub read_entry: ReadToolEntry,
    pub read_password_line: ReadToolEntry,
}

pub type ListEntryToolKeys =
    Arc<dyn Fn() -> Result<Vec<EntryToolKeySummary>, String> + Send + Sync>;
pub type EntryToolKeyRequiresUnlock = Arc<dyn Fn(String) -> Result<bool, String> + Send + Sync>;

#[derive(Clone)]
pub struct EntryToolKeyPorts {
    pub list_keys: ListEntryToolKeys,
    pub requires_session_unlock: EntryToolKeyRequiresUnlock,
    pub prompt_unlock: PromptEntryUnlock,
}

pub type RelevantStoreScopes = Arc<dyn Fn(String) -> Vec<String> + Send + Sync>;
pub type ReadStoreRecipients = Arc<dyn Fn(String, String) -> Vec<String> + Send + Sync>;

#[derive(Clone)]
pub struct EntryToolStorePorts {
    pub relevant_scopes: RelevantStoreScopes,
    pub read_standard_recipients: ReadStoreRecipients,
    pub root_scope: String,
}

#[derive(Clone)]
pub struct EntryToolUiPorts {
    pub preferences: EntryToolPreferencesPorts,
    pub backend: EntryToolBackendPorts,
    pub keys: EntryToolKeyPorts,
    pub stores: EntryToolStorePorts,
    pub show_root_page: WindowChromeCallback,
    pub refresh_tool_hub: Rc<dyn Fn()>,
}

impl EntryToolUiPorts {
    pub fn prompt_unlock(
        &self,
        overlay: &ToastOverlay,
        fingerprint: String,
        on_unlocked: Rc<dyn Fn()>,
        on_result: Rc<dyn Fn(bool)>,
    ) {
        (self.keys.prompt_unlock)(overlay, fingerprint, on_unlocked, on_result);
    }
}
