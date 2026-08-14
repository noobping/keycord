//! Backend-independent undo, move, and delete transactions for entries.

use crate::model::PassEntry;
use crate::{PasswordEntryError, PasswordEntryWriteError};

const UNAVAILABLE_UNDO_MESSAGE: &str = "Can't undo that change.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoAction {
    Unavailable {
        message: String,
    },
    RestoreSavedEntry {
        previous_store: String,
        previous_label: String,
        previous_contents: Option<String>,
        current_store: String,
        current_label: String,
    },
    RenameEntry {
        store: String,
        old_label: String,
        new_label: String,
    },
    MoveEntryBetweenStores {
        source_store: String,
        target_store: String,
        label: String,
    },
    RestoreDeletedEntry {
        store: String,
        label: String,
        contents: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UndoError {
    Read(PasswordEntryError),
    Write(PasswordEntryWriteError),
    Delete(PasswordEntryWriteError),
    Rename(PasswordEntryWriteError),
    Rollback {
        action_error: PasswordEntryWriteError,
        rollback_error: PasswordEntryWriteError,
    },
}

impl UndoError {
    pub fn toast_message(&self) -> &'static str {
        match self {
            Self::Read(err) => err.toast_message().unwrap_or("Can't undo the last change."),
            Self::Write(PasswordEntryWriteError::EntryAlreadyExists(_)) => {
                "An item with that name already exists."
            }
            Self::Write(PasswordEntryWriteError::MissingPrivateKey(_))
            | Self::Delete(PasswordEntryWriteError::MissingPrivateKey(_))
            | Self::Rename(PasswordEntryWriteError::MissingPrivateKey(_))
            | Self::Rollback {
                action_error: PasswordEntryWriteError::MissingPrivateKey(_),
                ..
            } => "Add a private key in Preferences.",
            Self::Write(PasswordEntryWriteError::LockedPrivateKey(_))
            | Self::Delete(PasswordEntryWriteError::LockedPrivateKey(_))
            | Self::Rename(PasswordEntryWriteError::LockedPrivateKey(_))
            | Self::Rollback {
                action_error: PasswordEntryWriteError::LockedPrivateKey(_),
                ..
            } => "Unlock the key in Preferences.",
            Self::Write(PasswordEntryWriteError::IncompatiblePrivateKey(_))
            | Self::Delete(PasswordEntryWriteError::IncompatiblePrivateKey(_))
            | Self::Rename(PasswordEntryWriteError::IncompatiblePrivateKey(_))
            | Self::Rollback {
                action_error: PasswordEntryWriteError::IncompatiblePrivateKey(_),
                ..
            } => "This key can't open your items.",
            Self::Delete(err) => err.delete_toast_message(),
            Self::Rename(err) => err.rename_toast_message(),
            Self::Write(_) | Self::Rollback { .. } => "Can't undo the last change.",
        }
    }
}

/// Backend entry operations used by transactional undo workflows.
pub trait EntryOperationPort: Send + Sync {
    fn read_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, PasswordEntryError>;

    fn save_password_entry(
        &self,
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError>;

    fn rename_password_entry(
        &self,
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), PasswordEntryWriteError>;

    fn delete_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<(), PasswordEntryWriteError>;
}

#[derive(Clone, Copy)]
pub struct EntryUndoBackend<'a> {
    operations: &'a dyn EntryOperationPort,
}

impl<'a> EntryUndoBackend<'a> {
    pub const fn new(operations: &'a dyn EntryOperationPort) -> Self {
        Self { operations }
    }

    pub fn delete_entry_with_optional_undo(
        self,
        entry: &PassEntry,
    ) -> Result<Option<UndoAction>, UndoError> {
        match self
            .operations
            .read_password_entry(&entry.store_path, &entry.label())
        {
            Ok(contents) => {
                self.operations
                    .delete_password_entry(&entry.store_path, &entry.label())
                    .map_err(UndoError::Delete)?;
                Ok(Some(restore_deleted_entry_action(entry, contents)))
            }
            Err(err) if can_delete_without_undo_after_read_error(&err) => {
                self.operations
                    .delete_password_entry(&entry.store_path, &entry.label())
                    .map_err(UndoError::Delete)?;
                Ok(Some(unavailable_undo_action()))
            }
            Err(err) => Err(UndoError::Read(err)),
        }
    }

    pub fn move_entry_to_store(
        self,
        entry: &PassEntry,
        target_store: &str,
    ) -> Result<PassEntry, UndoError> {
        let label = entry.label();
        self.move_entry_between_stores(&entry.store_path, target_store, &label)?;
        Ok(PassEntry::from_label(target_store.to_string(), &label))
    }

    pub fn execute_undo_action(self, action: &UndoAction) -> Result<(), UndoError> {
        match action {
            UndoAction::Unavailable { .. } => Ok(()),
            UndoAction::RestoreSavedEntry {
                previous_store,
                previous_label,
                previous_contents,
                current_store,
                current_label,
            } => self.restore_saved_entry(
                previous_store,
                previous_label,
                previous_contents.as_deref(),
                current_store,
                current_label,
            ),
            UndoAction::RenameEntry {
                store,
                old_label,
                new_label,
            } => self
                .operations
                .rename_password_entry(store, new_label, old_label)
                .map_err(UndoError::Rename),
            UndoAction::MoveEntryBetweenStores {
                source_store,
                target_store,
                label,
            } => self.move_entry_between_stores(target_store, source_store, label),
            UndoAction::RestoreDeletedEntry {
                store,
                label,
                contents,
            } => self
                .operations
                .save_password_entry(store, label, contents, false)
                .map_err(UndoError::Write),
        }
    }

    fn restore_saved_entry(
        self,
        previous_store: &str,
        previous_label: &str,
        previous_contents: Option<&str>,
        current_store: &str,
        current_label: &str,
    ) -> Result<(), UndoError> {
        let Some(previous_contents) = previous_contents else {
            return self
                .operations
                .delete_password_entry(current_store, current_label)
                .map_err(UndoError::Delete);
        };

        if previous_store == current_store && previous_label == current_label {
            return self
                .operations
                .save_password_entry(current_store, current_label, previous_contents, true)
                .map_err(UndoError::Write);
        }

        self.operations
            .save_password_entry(previous_store, previous_label, previous_contents, false)
            .map_err(UndoError::Write)?;

        if let Err(delete_error) = self
            .operations
            .delete_password_entry(current_store, current_label)
        {
            if let Err(rollback_error) = self
                .operations
                .delete_password_entry(previous_store, previous_label)
            {
                return Err(UndoError::Rollback {
                    action_error: delete_error,
                    rollback_error,
                });
            }
            return Err(UndoError::Delete(delete_error));
        }
        Ok(())
    }

    fn move_entry_between_stores(
        self,
        source_store: &str,
        target_store: &str,
        label: &str,
    ) -> Result<(), UndoError> {
        let contents = self
            .operations
            .read_password_entry(source_store, label)
            .map_err(UndoError::Read)?;
        self.operations
            .save_password_entry(target_store, label, &contents, false)
            .map_err(UndoError::Write)?;

        if let Err(delete_error) = self.operations.delete_password_entry(source_store, label) {
            if let Err(rollback_error) = self.operations.delete_password_entry(target_store, label)
            {
                return Err(UndoError::Rollback {
                    action_error: delete_error,
                    rollback_error,
                });
            }
            return Err(UndoError::Delete(delete_error));
        }
        Ok(())
    }
}

pub fn unavailable_undo_action() -> UndoAction {
    UndoAction::Unavailable {
        message: UNAVAILABLE_UNDO_MESSAGE.to_string(),
    }
}

pub fn unavailable_undo_message(action: &UndoAction) -> Option<&str> {
    match action {
        UndoAction::Unavailable { message } => Some(message.as_str()),
        UndoAction::RestoreSavedEntry { .. }
        | UndoAction::RenameEntry { .. }
        | UndoAction::MoveEntryBetweenStores { .. }
        | UndoAction::RestoreDeletedEntry { .. } => None,
    }
}

pub fn restore_deleted_entry_action(entry: &PassEntry, contents: String) -> UndoAction {
    UndoAction::RestoreDeletedEntry {
        store: entry.store_path.clone(),
        label: entry.label(),
        contents,
    }
}

pub fn restore_saved_entry_action(
    previous_store: &str,
    previous_label: &str,
    previous_contents: Option<&str>,
    current_store: &str,
    current_label: &str,
) -> UndoAction {
    UndoAction::RestoreSavedEntry {
        previous_store: previous_store.to_string(),
        previous_label: previous_label.to_string(),
        previous_contents: previous_contents.map(str::to_string),
        current_store: current_store.to_string(),
        current_label: current_label.to_string(),
    }
}

pub fn rename_entry_action(entry: &PassEntry, new_label: &str) -> UndoAction {
    UndoAction::RenameEntry {
        store: entry.store_path.clone(),
        old_label: entry.label(),
        new_label: new_label.to_string(),
    }
}

pub fn move_entry_between_stores_action(entry: &PassEntry, target_store: &str) -> UndoAction {
    UndoAction::MoveEntryBetweenStores {
        source_store: entry.store_path.clone(),
        target_store: target_store.to_string(),
        label: entry.label(),
    }
}

pub fn undo_action_restored_entry(action: &UndoAction) -> Option<(String, String)> {
    match action {
        UndoAction::Unavailable { .. } => None,
        UndoAction::RestoreSavedEntry {
            previous_store,
            previous_label,
            previous_contents,
            ..
        } => previous_contents
            .as_ref()
            .map(|_| (previous_store.clone(), previous_label.clone())),
        UndoAction::RenameEntry {
            store, old_label, ..
        } => Some((store.clone(), old_label.clone())),
        UndoAction::MoveEntryBetweenStores {
            source_store,
            label,
            ..
        } => Some((source_store.clone(), label.clone())),
        UndoAction::RestoreDeletedEntry { store, label, .. } => {
            Some((store.clone(), label.clone()))
        }
    }
}

fn can_delete_without_undo_after_read_error(error: &PasswordEntryError) -> bool {
    !matches!(error, PasswordEntryError::EntryNotFound(_))
}

#[cfg(test)]
mod tests {
    use super::{
        can_delete_without_undo_after_read_error, move_entry_between_stores_action,
        rename_entry_action, restore_deleted_entry_action, restore_saved_entry_action,
        unavailable_undo_action, unavailable_undo_message, undo_action_restored_entry,
        EntryOperationPort, EntryUndoBackend, UndoAction, UndoError,
    };
    use crate::model::PassEntry;
    use crate::{PasswordEntryError, PasswordEntryWriteError};
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct MemoryOperations {
        entries: Mutex<HashMap<(String, String), String>>,
        delete_failures: Mutex<HashMap<(String, String), PasswordEntryWriteError>>,
    }

    impl MemoryOperations {
        fn insert(&self, store: &str, label: &str, contents: &str) {
            self.entries
                .lock()
                .expect("entries lock")
                .insert((store.to_string(), label.to_string()), contents.to_string());
        }

        fn fail_delete(&self, store: &str, label: &str, message: &str) {
            self.delete_failures.lock().expect("failures lock").insert(
                (store.to_string(), label.to_string()),
                PasswordEntryWriteError::other(message),
            );
        }

        fn contains(&self, store: &str, label: &str) -> bool {
            self.entries
                .lock()
                .expect("entries lock")
                .contains_key(&(store.to_string(), label.to_string()))
        }
    }

    impl EntryOperationPort for MemoryOperations {
        fn read_password_entry(
            &self,
            store_root: &str,
            label: &str,
        ) -> Result<String, PasswordEntryError> {
            self.entries
                .lock()
                .expect("entries lock")
                .get(&(store_root.to_string(), label.to_string()))
                .cloned()
                .ok_or_else(|| PasswordEntryError::EntryNotFound("missing".to_string()))
        }

        fn save_password_entry(
            &self,
            store_root: &str,
            label: &str,
            contents: &str,
            overwrite: bool,
        ) -> Result<(), PasswordEntryWriteError> {
            let key = (store_root.to_string(), label.to_string());
            let mut entries = self.entries.lock().expect("entries lock");
            if entries.contains_key(&key) && !overwrite {
                return Err(PasswordEntryWriteError::already_exists("exists"));
            }
            entries.insert(key, contents.to_string());
            Ok(())
        }

        fn rename_password_entry(
            &self,
            store_root: &str,
            old_label: &str,
            new_label: &str,
        ) -> Result<(), PasswordEntryWriteError> {
            let mut entries = self.entries.lock().expect("entries lock");
            let value = entries
                .remove(&(store_root.to_string(), old_label.to_string()))
                .ok_or_else(|| PasswordEntryWriteError::entry_not_found("missing"))?;
            entries.insert((store_root.to_string(), new_label.to_string()), value);
            Ok(())
        }

        fn delete_password_entry(
            &self,
            store_root: &str,
            label: &str,
        ) -> Result<(), PasswordEntryWriteError> {
            let key = (store_root.to_string(), label.to_string());
            if let Some(error) = self
                .delete_failures
                .lock()
                .expect("failures lock")
                .remove(&key)
            {
                return Err(error);
            }
            self.entries
                .lock()
                .expect("entries lock")
                .remove(&key)
                .map(|_| ())
                .ok_or_else(|| PasswordEntryWriteError::entry_not_found("missing"))
        }
    }

    #[test]
    fn restored_entry_points_to_the_undone_location() {
        let action = UndoAction::MoveEntryBetweenStores {
            source_store: "/tmp/one".to_string(),
            target_store: "/tmp/two".to_string(),
            label: "work/github".to_string(),
        };
        assert_eq!(
            undo_action_restored_entry(&action),
            Some(("/tmp/one".to_string(), "work/github".to_string()))
        );
    }

    #[test]
    fn helper_actions_capture_the_original_location() {
        let entry = PassEntry::from_label("/tmp/store", "work/github");
        assert!(matches!(
            rename_entry_action(&entry, "work/gitlab"),
            UndoAction::RenameEntry { .. }
        ));
        assert!(matches!(
            move_entry_between_stores_action(&entry, "/tmp/other"),
            UndoAction::MoveEntryBetweenStores { .. }
        ));
        assert!(matches!(
            restore_deleted_entry_action(&entry, "secret".to_string()),
            UndoAction::RestoreDeletedEntry { .. }
        ));
        assert!(matches!(
            restore_saved_entry_action(
                "/tmp/store",
                "work/github",
                Some("secret"),
                "/tmp/store",
                "work/gitlab"
            ),
            UndoAction::RestoreSavedEntry { .. }
        ));
    }

    #[test]
    fn unavailable_undo_actions_expose_a_message() {
        let action = unavailable_undo_action();
        assert_eq!(
            unavailable_undo_message(&action),
            Some("Can't undo that change.")
        );
        assert_eq!(undo_action_restored_entry(&action), None);
    }

    #[test]
    fn delete_without_undo_is_allowed_for_any_read_failure_except_missing_entries() {
        assert!(can_delete_without_undo_after_read_error(
            &PasswordEntryError::missing_private_key("missing"),
        ));
        assert!(can_delete_without_undo_after_read_error(
            &PasswordEntryError::locked_private_key("locked"),
        ));
        assert!(can_delete_without_undo_after_read_error(
            &PasswordEntryError::incompatible_private_key("incompatible"),
        ));
        assert!(can_delete_without_undo_after_read_error(
            &PasswordEntryError::other("other"),
        ));
        assert!(!can_delete_without_undo_after_read_error(
            &PasswordEntryError::EntryNotFound("missing".to_string()),
        ));
    }

    #[test]
    fn move_rolls_back_target_when_source_delete_fails() {
        let operations = MemoryOperations::default();
        operations.insert("source", "team/item", "secret");
        operations.fail_delete("source", "team/item", "source delete failed");
        let backend = EntryUndoBackend::new(&operations);

        assert!(matches!(
            backend.move_entry_to_store(&PassEntry::from_label("source", "team/item"), "target"),
            Err(UndoError::Delete(_))
        ));
        assert!(operations.contains("source", "team/item"));
        assert!(!operations.contains("target", "team/item"));
    }

    #[test]
    fn move_reports_both_errors_when_rollback_fails() {
        let operations = MemoryOperations::default();
        operations.insert("source", "team/item", "secret");
        operations.fail_delete("source", "team/item", "source delete failed");
        operations.fail_delete("target", "team/item", "rollback failed");
        let backend = EntryUndoBackend::new(&operations);

        assert!(matches!(
            backend.move_entry_to_store(&PassEntry::from_label("source", "team/item"), "target"),
            Err(UndoError::Rollback { .. })
        ));
        assert!(operations.contains("target", "team/item"));
    }
}
