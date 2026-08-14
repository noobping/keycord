//! Linux Git action implementation.

use super::{set_git_operation_busy, GitActionState};
use crate::operations::{
    run_clone_operation_at_root, run_sync_operation_for_stores, GitOperationResult,
};
use crate::ui::store_page::{show_store_git_page, show_store_git_page_from_recipients};
use adw::Toast;
use keycord_runtime::{i18n::gettext, log_error};
use keycord_shell::actions::register_window_action;
use keycord_shell::background::spawn_result_task;
use keycord_shell::navigation::{
    finish_transient_navigation_page, push_navigation_page_if_needed, show_page_presentation,
    visible_navigation_page_is, HasWindowChrome, PagePresentation,
};
use keycord_stores::management::configured_store_for_shortcut_slot;
use keycord_stores::ui::management::NUMBERED_STORE_SHORTCUT_COUNT;
use std::rc::Rc;

pub fn clone_store_repository(url: &str, store_root: &str) -> Result<(), String> {
    match run_clone_operation_at_root(url, store_root) {
        GitOperationResult::Success => Ok(()),
        GitOperationResult::Failed(message) => Err(message),
    }
}

fn restore_after_git_operation(state: &GitActionState) {
    set_git_operation_busy(
        &state.window,
        state.ports.set_application_busy.as_ref(),
        false,
    );
    finish_transient_navigation_page(&state.nav, &state.busy_page);
    (state.ports.restore_navigation)();
}

fn restore_after_git_operation_and_refresh(state: &GitActionState) {
    restore_after_git_operation(state);
    (state.ports.refresh_after_operation)();
}

fn begin_git_operation(state: &GitActionState, title: &str) {
    set_git_operation_busy(
        &state.window,
        state.ports.set_application_busy.as_ref(),
        true,
    );
    show_page_presentation(
        &state.window_chrome(),
        &PagePresentation::secondary("Working", title, false),
    );
    state.busy_status.set_title(&gettext(title));
    push_navigation_page_if_needed(&state.nav, &state.busy_page);
}

fn register_cloned_store(state: &GitActionState, store: &str) -> Result<bool, String> {
    let mut stores = (state.ports.configured_stores)();
    if stores.iter().any(|configured| configured == store) {
        return Ok(false);
    }

    stores.push(store.to_string());
    (state.ports.set_configured_stores)(stores)?;
    Ok(true)
}

fn start_prompted_clone(state: &GitActionState, store: String, url: String) {
    begin_git_operation(state, "Restoring store");

    let state_for_result = state.clone();
    let state_for_disconnect = state.clone();
    let store_for_thread = store.clone();
    let store_for_result = store;
    spawn_result_task(
        move || clone_store_repository(&url, &store_for_thread),
        move |result| match result {
            Ok(()) => match register_cloned_store(&state_for_result, &store_for_result) {
                Ok(_) => {
                    restore_after_git_operation_and_refresh(&state_for_result);
                    state_for_result
                        .overlay
                        .add_toast(Toast::new(&gettext("Store restored.")));
                }
                Err(err) => {
                    restore_after_git_operation(&state_for_result);
                    log_error(format!("Failed to save stores: {err}"));
                    state_for_result
                        .overlay
                        .add_toast(Toast::new(&gettext("Couldn't add that folder.")));
                }
            },
            Err(message) => {
                restore_after_git_operation(&state_for_result);
                state_for_result
                    .overlay
                    .add_toast(Toast::new(&gettext(&message)));
            }
        },
        move || {
            restore_after_git_operation(&state_for_disconnect);
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Restore stopped unexpectedly.")));
        },
    );
}

pub fn register_open_git_action(state: &GitActionState) {
    let window = state.window.clone();
    let clone_state = state.clone();
    register_window_action(&window, "git-clone", move || {
        let on_submit: Rc<dyn Fn(String, String)> = Rc::new({
            let state = clone_state.clone();
            move |store, url| start_prompted_clone(&state, store, url)
        });
        (clone_state.ports.prompt_store_clone)(
            &clone_state.window,
            &clone_state.overlay,
            on_submit,
        );
    });

    let window = state.window.clone();
    let open_state = state.clone();
    register_window_action(&window, "open-git", move || {
        let on_submit: Rc<dyn Fn(String, String)> = Rc::new({
            let state = open_state.clone();
            move |store, url| start_prompted_clone(&state, store, url)
        });
        (open_state.ports.prompt_store_clone)(&open_state.window, &open_state.overlay, on_submit);
    });

    for slot in 1..=NUMBERED_STORE_SHORTCUT_COUNT {
        let action_window = state.window.clone();
        let state = state.clone();
        register_window_action(
            &action_window,
            &format!("open-store-git-{slot}"),
            move || {
                let stores = (state.ports.configured_stores)();
                let Some(store) = configured_store_for_shortcut_slot(&stores, slot) else {
                    return;
                };

                let preserve_recipients_state =
                    visible_navigation_page_is(&state.nav, &state.recipients_page.page)
                        && state
                            .recipients_page
                            .current_request()
                            .is_some_and(|request| request.store == store);
                if preserve_recipients_state {
                    show_store_git_page_from_recipients(&state.store_git_page, store);
                } else {
                    show_store_git_page(&state.store_git_page, store);
                }
            },
        );
    }
}

pub fn register_synchronize_action(state: &GitActionState) {
    let window = state.window.clone();
    let state = state.clone();
    register_window_action(&window, "synchronize", move || {
        begin_git_operation(&state, "Syncing stores");

        let state = state.clone();
        let state_for_disconnect = state.clone();
        let stores = (state.ports.configured_stores)();
        spawn_result_task(
            move || run_sync_operation_for_stores(&stores),
            move |result| match result {
                GitOperationResult::Success => {
                    restore_after_git_operation_and_refresh(&state);
                }
                GitOperationResult::Failed(message) => {
                    restore_after_git_operation_and_refresh(&state);
                    state.overlay.add_toast(Toast::new(&gettext(&message)));
                }
            },
            move || {
                restore_after_git_operation_and_refresh(&state_for_disconnect);
            },
        );
    });
}

pub fn handle_git_busy_back(state: &GitActionState) -> bool {
    if !visible_navigation_page_is(&state.nav, &state.busy_page) {
        return false;
    }

    state.nav.pop();
    (state.ports.restore_navigation)();
    true
}
