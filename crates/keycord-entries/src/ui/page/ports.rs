//! Composition ports for the password-page controller.

use std::rc::Rc;
use std::sync::{mpsc, Arc};

use adw::{ApplicationWindow, NavigationView, ToastOverlay};
use keycord_keys::PrivateKeyError;
use keycord_preferences::PasswordGenerationSettings;
use keycord_shell::navigation::WindowChromeCallback;
use secrecy::SecretString;

use super::super::list::EntryListUiPorts;
use crate::clipboard::PromptEntryUnlock;
use crate::{PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError};

pub type ReadEntryWithProgress = Arc<
    dyn Fn(
            String,
            String,
            mpsc::Sender<PasswordEntryReadProgress>,
        ) -> Result<String, PasswordEntryError>
        + Send
        + Sync,
>;
pub type SaveEntry =
    Arc<dyn Fn(String, String, String, bool) -> Result<(), PasswordEntryWriteError> + Send + Sync>;
pub type RenameEntry =
    Arc<dyn Fn(String, String, String) -> Result<(), PasswordEntryWriteError> + Send + Sync>;

#[derive(Clone)]
pub struct EntryPagePreferencesPorts {
    pub clear_empty_fields_before_save: Rc<dyn Fn() -> bool>,
    pub new_pass_file_template: Rc<dyn Fn() -> String>,
    pub default_store: Rc<dyn Fn() -> String>,
    pub password_generation_settings: Rc<dyn Fn() -> PasswordGenerationSettings>,
    pub uses_integrated_backend: Rc<dyn Fn() -> bool>,
    pub switch_to_integrated_backend: Rc<dyn Fn() -> Result<(), String>>,
    pub sync_private_keys_with_host: Rc<dyn Fn() -> bool>,
    pub disable_private_key_sync: Rc<dyn Fn() -> Result<(), String>>,
}

#[derive(Clone)]
pub struct EntryPageBackendPorts {
    pub read_entry_with_progress: ReadEntryWithProgress,
    pub save_entry: SaveEntry,
    pub rename_entry: RenameEntry,
}

pub type ResolveEntryFingerprint =
    Arc<dyn Fn(String, String) -> Result<String, String> + Send + Sync>;
pub type KeyRequiresPassphrase =
    Arc<dyn Fn(Vec<u8>) -> Result<bool, PrivateKeyError> + Send + Sync>;
pub type ImportPrivateKey =
    Arc<dyn Fn(Vec<u8>, Option<SecretString>) -> Result<(), PrivateKeyError> + Send + Sync>;
pub type SyncPrivateKeysToHost = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
pub type PromptPrivateKeyPassphrase =
    Rc<dyn Fn(&ApplicationWindow, &ToastOverlay, Rc<dyn Fn(SecretString)>)>;
pub type OpenPreferences = Rc<dyn Fn(&NavigationView)>;

#[derive(Clone)]
pub struct EntryPageKeyPorts {
    pub preferred_fingerprint: ResolveEntryFingerprint,
    pub prompt_unlock: PromptEntryUnlock,
    pub requires_passphrase: KeyRequiresPassphrase,
    pub import_private_key: ImportPrivateKey,
    pub sync_to_host: SyncPrivateKeysToHost,
    pub prompt_passphrase: PromptPrivateKeyPassphrase,
}

pub type PromptEntryGitUnlock = Rc<dyn Fn(&ToastOverlay, String, String, Rc<dyn Fn()>) -> bool>;

#[derive(Clone)]
pub struct EntryPageUiPorts {
    pub preferences: EntryPagePreferencesPorts,
    pub backend: EntryPageBackendPorts,
    pub keys: EntryPageKeyPorts,
    pub prompt_git_unlock: PromptEntryGitUnlock,
    pub open_preferences: OpenPreferences,
    pub list: EntryListUiPorts,
    pub show_root_page: WindowChromeCallback,
    pub sync_tools_action_availability: Rc<dyn Fn(&ApplicationWindow)>,
}
