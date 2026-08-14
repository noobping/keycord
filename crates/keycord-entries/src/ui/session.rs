//! Per-window entry selection and undo history.

use crate::model::OpenPassFile;
use crate::undo::UndoAction;
use adw::gtk::Widget;
use adw::prelude::*;
use adw::ApplicationWindow;
use keycord_runtime::log_error;
use keycord_shell::object_data::{cloned_data, set_cloned_data};
use std::cell::RefCell;
use std::rc::Rc;

const MAX_UNDO_ACTIONS: usize = 32;
const WINDOW_SESSION_STATE_KEY: &str = "window-session-state";

/// Entries-owned state that must be isolated between application windows.
#[derive(Clone, Default)]
pub struct EntrySessionState {
    opened_pass_file: Rc<RefCell<Option<OpenPassFile>>>,
    undo_stack: Rc<RefCell<Vec<UndoAction>>>,
}

/// Installs a fresh Entries session on an application window.
pub fn initialize_window_session(window: &ApplicationWindow) -> EntrySessionState {
    let session = EntrySessionState::default();
    set_cloned_data(window, WINDOW_SESSION_STATE_KEY, session.clone());
    session
}

pub fn window_session(window: &ApplicationWindow) -> Option<EntrySessionState> {
    cloned_data(window, WINDOW_SESSION_STATE_KEY)
}

pub fn window_session_for_widget(widget: &impl IsA<Widget>) -> Option<EntrySessionState> {
    let window = widget
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok());
    let Some(window) = window else {
        log_error("Widget was not attached to an application window when reading window session.");
        return None;
    };
    let session = window_session(&window);
    if session.is_none() {
        log_error("Window session was not initialized before use.");
    }
    session
}

impl EntrySessionState {
    pub fn set_opened_pass_file(&self, pass_file: OpenPassFile) {
        *self.opened_pass_file.borrow_mut() = Some(pass_file);
    }

    pub fn get_opened_pass_file(&self) -> Option<OpenPassFile> {
        self.opened_pass_file.borrow().clone()
    }

    pub fn clear_opened_pass_file(&self) {
        *self.opened_pass_file.borrow_mut() = None;
    }

    pub fn is_opened_pass_file(&self, pass_file: &OpenPassFile) -> bool {
        self.opened_pass_file.borrow().as_ref() == Some(pass_file)
    }

    pub fn refresh_opened_pass_file_from_contents(
        &self,
        pass_file: &OpenPassFile,
        contents: &str,
    ) -> Option<OpenPassFile> {
        let mut opened_pass_file = self.opened_pass_file.borrow_mut();
        let selected = opened_pass_file.as_mut()?;
        if selected != pass_file {
            return None;
        }

        selected.refresh_from_contents(contents);
        Some(selected.clone())
    }

    pub fn push_undo_action(&self, action: UndoAction) {
        let mut undo_stack = self.undo_stack.borrow_mut();
        undo_stack.push(action);
        if undo_stack.len() > MAX_UNDO_ACTIONS {
            let drain_len = undo_stack.len() - MAX_UNDO_ACTIONS;
            undo_stack.drain(0..drain_len);
        }
    }

    pub fn pop_undo_action(&self) -> Option<UndoAction> {
        self.undo_stack.borrow_mut().pop()
    }

    #[cfg(test)]
    pub fn has_undo_actions(&self) -> bool {
        !self.undo_stack.borrow().is_empty()
    }

    #[cfg(test)]
    pub fn clear_undo_actions(&self) {
        self.undo_stack.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::{EntrySessionState, MAX_UNDO_ACTIONS};
    use crate::model::OpenPassFile;
    use crate::undo::UndoAction;
    use keycord_preferences::UsernameFallbackMode;

    #[test]
    fn sessions_keep_opened_pass_files_separate() {
        let first = EntrySessionState::default();
        let second = EntrySessionState::default();

        let first_pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/first",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        let second_pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/second",
            "work/bob/gitlab",
            UsernameFallbackMode::Folder,
        );

        first.set_opened_pass_file(first_pass_file.clone());
        second.set_opened_pass_file(second_pass_file.clone());

        assert_eq!(first.get_opened_pass_file(), Some(first_pass_file));
        assert_eq!(second.get_opened_pass_file(), Some(second_pass_file));
    }

    #[test]
    fn sessions_keep_undo_stacks_separate() {
        let first = EntrySessionState::default();
        let second = EntrySessionState::default();

        first.push_undo_action(UndoAction::RenameEntry {
            store: "/tmp/first".to_string(),
            old_label: "work/alice/github".to_string(),
            new_label: "work/alice/gitlab".to_string(),
        });
        second.push_undo_action(UndoAction::RenameEntry {
            store: "/tmp/second".to_string(),
            old_label: "work/bob/gitlab".to_string(),
            new_label: "work/bob/github".to_string(),
        });

        assert!(first.has_undo_actions());
        assert!(second.has_undo_actions());
        assert_eq!(
            first.pop_undo_action(),
            Some(UndoAction::RenameEntry {
                store: "/tmp/first".to_string(),
                old_label: "work/alice/github".to_string(),
                new_label: "work/alice/gitlab".to_string(),
            })
        );
        assert_eq!(
            second.pop_undo_action(),
            Some(UndoAction::RenameEntry {
                store: "/tmp/second".to_string(),
                old_label: "work/bob/gitlab".to_string(),
                new_label: "work/bob/github".to_string(),
            })
        );
    }

    #[test]
    fn undo_history_is_bounded() {
        let session = EntrySessionState::default();
        for index in 0..40 {
            session.push_undo_action(UndoAction::RenameEntry {
                store: "/tmp/store".to_string(),
                old_label: format!("old-{index}"),
                new_label: format!("new-{index}"),
            });
        }

        let mut popped = 0;
        while session.pop_undo_action().is_some() {
            popped += 1;
        }
        assert_eq!(popped, MAX_UNDO_ACTIONS);
    }
}
