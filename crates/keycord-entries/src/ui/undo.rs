//! GTK adapters and controller for the Entries-owned undo stack.

use adw::gtk::Widget;
use adw::prelude::*;
use adw::Toast;
use keycord_runtime::i18n::gettext;
use keycord_shell::background::spawn_result_task;
use keycord_shell::ui::visible_navigation_page_is;
use std::rc::Rc;
use std::sync::Arc;

use super::list::PasswordListVisibilityState;
use super::page::{
    open_password_entry_page, password_page_has_unsaved_changes, revert_unsaved_password_changes,
    show_password_list_page, PasswordPageState,
};
use super::session::window_session_for_widget;
use crate::model::OpenPassFile;
use crate::undo::{unavailable_undo_message, undo_action_restored_entry, UndoAction, UndoError};

pub type ExecuteUndoCallback = dyn Fn(&UndoAction) -> Result<(), UndoError> + Send + Sync + 'static;
pub type UndoUiCallback = dyn Fn() + 'static;

#[derive(Clone)]
pub struct ContextUndoPorts {
    execute_undo: Arc<ExecuteUndoCallback>,
    reload_password_list: Rc<UndoUiCallback>,
    restore_navigation: Rc<UndoUiCallback>,
}

impl ContextUndoPorts {
    pub fn new(
        execute_undo: Arc<ExecuteUndoCallback>,
        reload_password_list: Rc<UndoUiCallback>,
        restore_navigation: Rc<UndoUiCallback>,
    ) -> Self {
        Self {
            execute_undo,
            reload_password_list,
            restore_navigation,
        }
    }
}

#[derive(Clone)]
pub struct ContextUndoActionState {
    password_page: PasswordPageState,
    visibility: PasswordListVisibilityState,
    ports: ContextUndoPorts,
}

impl ContextUndoActionState {
    pub fn new(
        password_page: &PasswordPageState,
        visibility: &PasswordListVisibilityState,
        ports: ContextUndoPorts,
    ) -> Self {
        Self {
            password_page: password_page.clone(),
            visibility: visibility.clone(),
            ports,
        }
    }
}

pub fn push_undo_action(widget: &impl IsA<Widget>, action: UndoAction) {
    if let Some(session) = window_session_for_widget(widget) {
        session.push_undo_action(action);
    }
}

pub fn pop_undo_action(widget: &impl IsA<Widget>) -> Option<UndoAction> {
    window_session_for_widget(widget).and_then(|session| session.pop_undo_action())
}

fn password_editor_is_visible(state: &PasswordPageState) -> bool {
    visible_navigation_page_is(&state.nav, &state.page)
        || visible_navigation_page_is(&state.nav, &state.raw_page)
}

pub fn context_undo_callback(state: &ContextUndoActionState) -> Rc<dyn Fn()> {
    let state = state.clone();
    Rc::new(move || {
        let editing_password = password_editor_is_visible(&state.password_page);
        if editing_password && password_page_has_unsaved_changes(&state.password_page) {
            let _ = revert_unsaved_password_changes(&state.password_page);
            return;
        }

        let Some(action) = pop_undo_action(&state.password_page.nav) else {
            return;
        };
        if let Some(message) = unavailable_undo_message(&action) {
            state
                .password_page
                .overlay
                .add_toast(Toast::new(&gettext(message)));
            return;
        }

        let overlay = state.password_page.overlay.clone();
        let state_for_result = state.clone();
        let state_for_disconnect = state.clone();
        let action_for_task = action.clone();
        let action_for_result = action.clone();
        let action_for_disconnect = action;
        let execute_undo = state.ports.execute_undo.clone();
        spawn_result_task(
            move || execute_undo(&action_for_task),
            move |result| match result {
                Ok(()) => {
                    if editing_password {
                        if let Some((store, label)) = undo_action_restored_entry(&action_for_result)
                        {
                            open_password_entry_page(
                                &state_for_result.password_page,
                                OpenPassFile::from_label(store, &label),
                                false,
                            );
                        } else {
                            show_password_list_page(
                                &state_for_result.password_page,
                                state_for_result.visibility.show_hidden(),
                                state_for_result.visibility.show_duplicates(),
                            );
                        }
                    } else {
                        (state_for_result.ports.reload_password_list)();
                        (state_for_result.ports.restore_navigation)();
                    }
                    overlay.add_toast(Toast::new(&gettext("Undone.")));
                }
                Err(err) => {
                    push_undo_action(&state_for_result.password_page.nav, action_for_result);
                    overlay.add_toast(Toast::new(&gettext(err.toast_message())));
                }
            },
            move || {
                push_undo_action(
                    &state_for_disconnect.password_page.nav,
                    action_for_disconnect,
                );
                state_for_disconnect
                    .password_page
                    .overlay
                    .add_toast(Toast::new(&gettext("Can't undo the last change.")));
            },
        );
    })
}
