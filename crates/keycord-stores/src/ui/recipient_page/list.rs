//! Stores-owned recipient scope, selection policy, and autosave wiring.

use super::mode::{
    current_selection_mode, show_standard_private_key_choice, sync_store_recipients_mode_controls,
};
use super::{
    load_store_recipients_scope, queue_store_recipients_autosave, StoreRecipientsPageState,
};
use crate::recipient_page::{
    private_key_delete_block_message, private_key_toggle_block_message, recipient_matches_parts,
    recipient_scope_label, set_private_key_recipient_values, show_recipient_scope_selector,
    show_require_all_private_keys_option, show_store_options_title_above_git_row,
};
use crate::{
    relevant_store_recipient_scopes, StoreRecipientsPrivateKeyRequirement,
    ROOT_STORE_RECIPIENTS_SCOPE,
};
use adw::gtk::StringList;
use adw::prelude::*;
use adw::ActionRow;
use keycord_keys::ui::{RecipientKeyListContext, RecipientKeyListPolicy};
use keycord_runtime::i18n::gettext;
use keycord_shell::ui::{dim_label_icon, flat_icon_button_with_tooltip};
use std::rc::Rc;

fn set_private_key_requirement(
    state: &StoreRecipientsPageState,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> bool {
    let changed = state.private_key_requirement.get() != private_key_requirement;
    if changed {
        state.private_key_requirement.set(private_key_requirement);
    }
    changed
}

fn append_unresolved_recipient_rows(state: &StoreRecipientsPageState, recipients: &[String]) {
    for recipient in recipients {
        let row = ActionRow::builder()
            .title(recipient)
            .subtitle(gettext("This recipient is not available in the app."))
            .build();
        row.set_activatable(false);
        row.add_prefix(&dim_label_icon("dialog-warning-symbolic"));

        let delete_button =
            flat_icon_button_with_tooltip("user-trash-symbolic", "Remove recipient");
        row.add_suffix(&delete_button);
        state.key_management.append_recipient_projection_row(&row);

        let page_state = state.clone();
        let recipient = recipient.clone();
        delete_button.connect_clicked(move |_| {
            let before = page_state.recipients.borrow().clone();
            page_state
                .recipients
                .borrow_mut()
                .retain(|value| value != &recipient);
            super::rebuild_store_recipients_list(&page_state);
            if *page_state.recipients.borrow() != before {
                queue_store_recipients_autosave(&page_state);
            }
        });
    }
}

fn available_recipient_scopes(state: &StoreRecipientsPageState) -> Vec<String> {
    let Some(request) = state.current_request() else {
        return vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()];
    };
    if !state.ports.preferences.uses_integrated_backend() {
        return vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()];
    }

    let scopes = relevant_store_recipient_scopes(&request.store);
    if scopes.is_empty() {
        vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()]
    } else {
        scopes
    }
}

fn scope_row_model(scopes: &[String]) -> StringList {
    let labels = scopes
        .iter()
        .map(|scope| recipient_scope_label(scope))
        .collect::<Vec<_>>();
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    StringList::new(&label_refs)
}

fn sync_recipient_group_headers(state: &StoreRecipientsPageState, show_scope_selector: bool) {
    state.platform.scope_group.set_visible(show_scope_selector);
    state
        .key_management
        .sync_recipient_group_header(show_scope_selector);
}

pub(super) fn sync_store_recipients_busy_indicator(state: &StoreRecipientsPageState) {
    state
        .platform
        .saving_group
        .set_visible(state.save_in_flight.get());
}

fn sync_recipient_scope_row(state: &StoreRecipientsPageState) {
    let scopes = available_recipient_scopes(state);
    let current_scope = state.current_recipient_scope();
    let selected_scope = if scopes.iter().any(|scope| scope == &current_scope) {
        current_scope
    } else {
        scopes
            .first()
            .cloned()
            .unwrap_or_else(|| ROOT_STORE_RECIPIENTS_SCOPE.to_string())
    };
    let show_scope_selector = show_recipient_scope_selector(&scopes);
    let scopes_changed = *state.recipient_scope_dirs.borrow() != scopes;

    if scopes_changed {
        *state.recipient_scope_dirs.borrow_mut() = scopes.clone();
        let model = scope_row_model(&scopes);
        state.platform.scope_row.set_model(Some(&model));
    }
    sync_recipient_group_headers(state, show_scope_selector);
    state.platform.scope_list.set_visible(show_scope_selector);
    state.platform.scope_row.set_visible(show_scope_selector);
    state.platform.scope_row.set_sensitive(show_scope_selector);

    let selected_position = scopes
        .iter()
        .position(|scope| scope == &selected_scope)
        .unwrap_or(0);
    if state.platform.scope_row.selected() != selected_position as u32 {
        state
            .platform
            .scope_row
            .set_selected(selected_position as u32);
    }

    if selected_scope != state.current_recipient_scope() {
        let Some(request) = state.current_request() else {
            return;
        };
        load_store_recipients_scope(state, &request.store, &selected_scope);
    }
}

pub(super) fn refresh_recipient_scope_row(state: &StoreRecipientsPageState) {
    sync_recipient_scope_row(state);
}

fn sync_private_key_requirement_row(state: &StoreRecipientsPageState, has_keys: bool) {
    let selection_mode = current_selection_mode(state);
    let uses_integrated_backend = state.ports.preferences.uses_integrated_backend();
    let show_require_all = show_require_all_private_keys_option(selection_mode, has_keys);
    let show_store_options_title = show_store_options_title_above_git_row(
        show_require_all,
        state.platform.git_group.get_visible(),
    );
    let git_group_title = if show_store_options_title {
        gettext("Experimental store options")
    } else {
        String::new()
    };

    state.platform.options_group.set_visible(show_require_all);
    state.platform.git_group.set_title(&git_group_title);
    state.platform.require_all_row.set_visible(show_require_all);
    state
        .platform
        .require_all_row
        .set_sensitive(show_require_all && uses_integrated_backend);
    state
        .platform
        .require_all_check
        .set_sensitive(show_require_all && uses_integrated_backend);
    state.platform.require_all_check.set_active(matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    ));
}

pub(super) fn connect_private_key_requirement_control(state: &StoreRecipientsPageState) {
    let row = state.platform.require_all_row.clone();
    let check = state.platform.require_all_check.clone();
    let check_for_row = check.clone();
    row.connect_activated(move |_| {
        check_for_row.set_active(!check_for_row.is_active());
    });

    let page_state = state.clone();
    check.connect_toggled(move |button| {
        let private_key_requirement = if button.is_active() {
            StoreRecipientsPrivateKeyRequirement::AllManagedKeys
        } else {
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey
        };
        if set_private_key_requirement(&page_state, private_key_requirement) {
            super::rebuild_store_recipients_list(&page_state);
            queue_store_recipients_autosave(&page_state);
        }
    });
}

pub(super) fn connect_recipient_scope_control(state: &StoreRecipientsPageState) {
    let row = state.platform.scope_row.clone();
    let page_state = state.clone();
    row.connect_selected_notify(move |row| {
        let Some(request) = page_state.current_request() else {
            return;
        };
        let scopes = page_state.recipient_scope_dirs.borrow().clone();
        let Some(scope) = scopes.get(row.selected() as usize).cloned() else {
            return;
        };
        if scope == page_state.current_recipient_scope() {
            return;
        }

        load_store_recipients_scope(&page_state, &request.store, &scope);
        super::rebuild_store_recipients_list(&page_state);
    });
}

pub(super) fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    (state.ports.git.rebuild_recipient_row)(state);
    let _ = state.key_management.refresh_recipient_key_inventory();
    sync_store_recipients_busy_indicator(state);
    sync_recipient_scope_row(state);

    let current_recipients = state.recipients.borrow().clone();
    let uses_integrated_backend = state.ports.preferences.uses_integrated_backend();
    let uses_host_backend = state.ports.preferences.uses_host_command_backend();
    let selection_mode = current_selection_mode(state);
    sync_store_recipients_mode_controls(state, selection_mode, uses_integrated_backend);
    let require_all = matches!(
        state.private_key_requirement.get(),
        StoreRecipientsPrivateKeyRequirement::AllManagedKeys
    );

    let state_for_toggle = state.clone();
    let state_before_rows = state.clone();
    state
        .key_management
        .rebuild_recipient_key_list(RecipientKeyListContext {
            current_recipients,
            uses_integrated_backend,
            uses_host_backend,
            policy: RecipientKeyListPolicy {
                recipient_matches: Rc::new(recipient_matches_parts),
                show_choice: Rc::new(move |active| {
                    show_standard_private_key_choice(selection_mode, active)
                }),
                toggle_blocked_message: Rc::new(
                    move |active, usable, selected, usable_selected| {
                        private_key_toggle_block_message(
                            active,
                            usable,
                            require_all,
                            selected,
                            usable_selected,
                        )
                        .map(str::to_string)
                    },
                ),
                delete_blocked_message: Rc::new(move |active, selected| {
                    private_key_delete_block_message(active, require_all, selected)
                        .map(str::to_string)
                }),
                on_toggle: Rc::new(move |fingerprint, user_ids, enabled| {
                    if set_private_key_recipient_values(
                        &mut state_for_toggle.recipients.borrow_mut(),
                        &fingerprint,
                        &user_ids,
                        enabled,
                    ) {
                        super::rebuild_store_recipients_list(&state_for_toggle);
                        queue_store_recipients_autosave(&state_for_toggle);
                    }
                }),
                before_key_rows: Rc::new(move |has_keys, unresolved| {
                    sync_private_key_requirement_row(&state_before_rows, has_keys);
                    append_unresolved_recipient_rows(&state_before_rows, &unresolved);
                }),
            },
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recipient_page::StoreRecipientsSelectionMode;

    #[test]
    fn require_all_option_is_available_when_keys_exist() {
        assert!(!show_require_all_private_keys_option(
            StoreRecipientsSelectionMode::Empty,
            false
        ));
        assert!(show_require_all_private_keys_option(
            StoreRecipientsSelectionMode::StandardOnly,
            true
        ));
    }

    #[test]
    fn git_row_shows_store_options_title_when_it_is_the_only_option() {
        assert!(show_store_options_title_above_git_row(false, true));
        assert!(!show_store_options_title_above_git_row(true, true));
        assert!(!show_store_options_title_above_git_row(false, false));
    }

    #[test]
    fn recipient_scope_selector_only_shows_for_multiple_relevant_scopes() {
        assert!(!show_recipient_scope_selector(&[".".to_string()]));
        assert!(show_recipient_scope_selector(&[
            ".".to_string(),
            "team".to_string(),
        ]));
    }

    #[test]
    fn root_recipient_scope_uses_default_label() {
        assert_eq!(recipient_scope_label("."), "Default".to_string());
        assert_eq!(recipient_scope_label("team"), "team".to_string());
    }
}
