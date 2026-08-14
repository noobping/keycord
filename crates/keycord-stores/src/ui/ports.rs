//! Composition ports for store-specific UI.

use std::rc::Rc;
use std::sync::Arc;

use adw::{ApplicationWindow, ToastOverlay};
use secrecy::SecretString;

use super::recipient_page::StoreRecipientsPageState;
use crate::{StoreRecipients, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement};

#[derive(Clone, Debug)]
pub struct StorePassImportRequest {
    pub store_root: String,
    pub source: String,
    pub source_path: Option<String>,
    pub source_password: SecretString,
    pub target_path: Option<String>,
}

#[derive(Clone)]
pub struct StorePreferencesPorts {
    stores: Rc<dyn Fn() -> Vec<String>>,
    set_stores: Rc<dyn Fn(Vec<String>) -> Result<(), String>>,
    prune_missing_stores: Rc<dyn Fn() -> Result<bool, String>>,
    uses_integrated_backend: Rc<dyn Fn() -> bool>,
    uses_host_command_backend: Rc<dyn Fn() -> bool>,
}

impl StorePreferencesPorts {
    pub fn new(
        stores: impl Fn() -> Vec<String> + 'static,
        set_stores: impl Fn(Vec<String>) -> Result<(), String> + 'static,
        prune_missing_stores: impl Fn() -> Result<bool, String> + 'static,
        uses_integrated_backend: impl Fn() -> bool + 'static,
        uses_host_command_backend: impl Fn() -> bool + 'static,
    ) -> Self {
        Self {
            stores: Rc::new(stores),
            set_stores: Rc::new(set_stores),
            prune_missing_stores: Rc::new(prune_missing_stores),
            uses_integrated_backend: Rc::new(uses_integrated_backend),
            uses_host_command_backend: Rc::new(uses_host_command_backend),
        }
    }

    pub fn stores(&self) -> Vec<String> {
        (self.stores)()
    }

    pub fn set_stores(&self, stores: Vec<String>) -> Result<(), String> {
        (self.set_stores)(stores)
    }

    pub fn prune_missing_stores(&self) -> Result<bool, String> {
        (self.prune_missing_stores)()
    }

    pub fn uses_integrated_backend(&self) -> bool {
        (self.uses_integrated_backend)()
    }

    pub fn uses_host_command_backend(&self) -> bool {
        (self.uses_host_command_backend)()
    }
}

pub type SaveStoreRecipients = Arc<
    dyn Fn(
            String,
            String,
            StoreRecipients,
            StoreRecipientsPrivateKeyRequirement,
        ) -> Result<(), StoreRecipientsError>
        + Send
        + Sync,
>;
pub type StoreRecipientUnlockFingerprint =
    Arc<dyn Fn(String, String) -> Result<Option<String>, String> + Send + Sync>;

#[derive(Clone)]
pub struct StoreBackendUiPorts {
    pub save_recipients: SaveStoreRecipients,
    pub recipient_unlock_fingerprint: StoreRecipientUnlockFingerprint,
}

pub type CloneStoreRepository = Arc<dyn Fn(String, String) -> Result<(), String> + Send + Sync>;
pub type RebuildStoreRecipientGitRow = Rc<dyn Fn(&StoreRecipientsPageState)>;
pub type PromptStoreGitUnlock = Rc<
    dyn Fn(
        &ToastOverlay,
        &str,
        &StoreRecipients,
        StoreRecipientsPrivateKeyRequirement,
        &Rc<dyn Fn()>,
    ) -> bool,
>;

#[derive(Clone)]
pub struct StoreGitUiPorts {
    pub clone_repository: CloneStoreRepository,
    pub rebuild_recipient_row: RebuildStoreRecipientGitRow,
    pub prompt_recipient_commit_unlock: PromptStoreGitUnlock,
}

pub type AvailableStoreImportSources = Arc<dyn Fn() -> Result<Vec<String>, String> + Send + Sync>;
pub type RunStoreImport = Arc<dyn Fn(StorePassImportRequest) -> Result<(), String> + Send + Sync>;

#[derive(Clone)]
pub struct StoreImportUiPorts {
    pub available_sources: AvailableStoreImportSources,
    pub run_import: RunStoreImport,
}

#[derive(Clone)]
pub struct StoreNavigationUiPorts {
    pub show_secondary_page: ShowStoreSecondaryPage,
}

pub type ShowStoreSecondaryPage = Rc<dyn Fn(&str, &str, bool)>;

#[derive(Clone)]
pub struct StoreRefreshUiPorts {
    pub reload_password_list: Rc<dyn Fn(&ApplicationWindow)>,
    pub reload_store_recipients: Rc<dyn Fn(&ApplicationWindow)>,
}

#[derive(Clone)]
pub struct StoreUiPorts {
    pub preferences: StorePreferencesPorts,
    pub backend: StoreBackendUiPorts,
    pub git: StoreGitUiPorts,
    pub import: StoreImportUiPorts,
    pub navigation: StoreNavigationUiPorts,
    pub refresh: StoreRefreshUiPorts,
}
