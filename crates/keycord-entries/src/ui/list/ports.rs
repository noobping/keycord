//! Composition ports for the password-list controller.

use std::rc::Rc;
use std::sync::Arc;

use adw::{Application, ApplicationWindow};
use keycord_preferences::PasswordListSortMode;

use crate::clipboard::EntryClipboardPorts;
use crate::model::{CollectItemsOptions, PassEntry};
use crate::undo::{UndoAction, UndoError};
use crate::PasswordEntryWriteError;

pub type CollectEntryItems = Arc<dyn Fn(CollectItemsOptions) -> Vec<PassEntry> + Send + Sync>;
pub type EntryIsReadable = Arc<dyn Fn(String, String) -> bool + Send + Sync>;
pub type ReadEntryForSearch = Arc<dyn Fn(String, String) -> Result<String, String> + Send + Sync>;
pub type RenameEntry =
    Arc<dyn Fn(String, String, String) -> Result<(), PasswordEntryWriteError> + Send + Sync>;
pub type DeleteEntryWithUndo =
    Arc<dyn Fn(PassEntry) -> Result<Option<UndoAction>, UndoError> + Send + Sync>;
pub type MoveEntryToStore =
    Arc<dyn Fn(PassEntry, String) -> Result<PassEntry, UndoError> + Send + Sync>;
pub type OpenEntryWindow = Rc<dyn Fn(&Application, PassEntry) -> Result<ApplicationWindow, String>>;
pub type StoreGitSummary = Arc<dyn Fn(String) -> String + Send + Sync>;

#[derive(Clone)]
pub struct EntryListPreferencesPorts {
    pub stores: Rc<dyn Fn() -> Vec<String>>,
    pub store_roots: Rc<dyn Fn() -> Vec<String>>,
    pub prune_missing_stores: Rc<dyn Fn() -> Result<(), String>>,
    pub included_store_roots: Rc<dyn Fn() -> Option<Vec<String>>>,
    pub set_included_store_roots: Rc<dyn Fn(Vec<String>) -> Result<(), String>>,
    pub sort_mode: Rc<dyn Fn() -> PasswordListSortMode>,
}

#[derive(Clone)]
pub struct EntryListBackendPorts {
    pub collect_items: CollectEntryItems,
    pub is_readable: EntryIsReadable,
    pub read_entry: ReadEntryForSearch,
    pub rename_entry: RenameEntry,
    pub delete_entry_with_undo: DeleteEntryWithUndo,
    pub move_entry_to_store: MoveEntryToStore,
}

#[derive(Clone)]
pub struct EntryListUiPorts {
    pub preferences: EntryListPreferencesPorts,
    pub backend: EntryListBackendPorts,
    pub clipboard: EntryClipboardPorts,
    pub open_entry_window: OpenEntryWindow,
    pub store_git_summary: StoreGitSummary,
    pub app_id: String,
}

impl EntryListUiPorts {
    pub fn read_entry_for_search(&self) -> ReadEntryForSearch {
        self.backend.read_entry.clone()
    }
}
