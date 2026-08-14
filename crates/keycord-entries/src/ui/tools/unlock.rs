use super::{EntryToolKeyRequiresUnlock, EntryToolKeySummary, EntryToolsState, FieldValueRequest};
use crate::clipboard::PromptEntryUnlock;
use adw::{Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_shell::background::spawn_result_task;
use std::rc::Rc;

impl EntryToolsState {
    pub(super) fn unlock_tool_keys_if_needed(
        &self,
        requests: Vec<FieldValueRequest>,
        on_ready: Rc<dyn Fn(Vec<FieldValueRequest>)>,
        on_abort: Rc<dyn Fn()>,
    ) {
        if !(self.ports.preferences.uses_integrated_backend)() {
            on_ready(requests);
            return;
        }

        let requests_for_unlock = requests.clone();
        let on_ready_for_result = on_ready.clone();
        let on_abort_for_result = on_abort.clone();
        let overlay_for_result = self.overlay.clone();
        let overlay_for_disconnect = self.overlay.clone();
        let list_keys = self.ports.keys.list_keys.clone();
        let requires_unlock = self.ports.keys.requires_session_unlock.clone();
        let relevant_scopes = self.ports.stores.relevant_scopes.clone();
        let read_recipients = self.ports.stores.read_standard_recipients.clone();
        let root_scope = self.ports.stores.root_scope.clone();
        let prompt_unlock = self.ports.keys.prompt_unlock.clone();
        spawn_result_task(
            move || {
                collect_locked_tool_fingerprints(
                    &requests_for_unlock,
                    &list_keys,
                    &requires_unlock,
                    &relevant_scopes,
                    &read_recipients,
                    &root_scope,
                )
            },
            move |fingerprints| {
                if fingerprints.is_empty() {
                    on_ready_for_result(requests);
                    return;
                }

                let on_abort_for_unlock = on_abort_for_result.clone();
                prompt_tool_unlock_sequence(
                    &overlay_for_result,
                    fingerprints,
                    prompt_unlock,
                    Rc::new(move |success| {
                        if success {
                            on_ready(requests.clone());
                        } else {
                            on_abort_for_unlock();
                        }
                    }),
                );
            },
            move || {
                on_abort();
                overlay_for_disconnect
                    .add_toast(Toast::new(&gettext("Couldn't prepare tool access.")));
            },
        );
    }
}

fn collect_locked_tool_fingerprints(
    requests: &[FieldValueRequest],
    list_keys: &super::ListEntryToolKeys,
    requires_unlock: &EntryToolKeyRequiresUnlock,
    relevant_scopes: &super::RelevantStoreScopes,
    read_recipients: &super::ReadStoreRecipients,
    root_scope: &str,
) -> Vec<String> {
    collect_unlockable_standard_tool_fingerprints(
        requests,
        list_keys,
        requires_unlock,
        relevant_scopes,
        read_recipients,
        root_scope,
    )
}

fn collect_unlockable_standard_tool_fingerprints(
    requests: &[FieldValueRequest],
    list_keys: &super::ListEntryToolKeys,
    requires_unlock: &EntryToolKeyRequiresUnlock,
    relevant_scopes: &super::RelevantStoreScopes,
    read_recipients: &super::ReadStoreRecipients,
    root_scope: &str,
) -> Vec<String> {
    let Ok(keys) = available_tool_keys(list_keys) else {
        return Vec::new();
    };
    let recipients =
        collect_tool_standard_recipients(requests, relevant_scopes, read_recipients, root_scope);
    let mut fingerprints = Vec::new();

    for key in keys {
        if recipients
            .iter()
            .any(|recipient| tool_recipient_matches_key(recipient, &key))
        {
            append_unlockable_tool_fingerprints(
                &mut fingerprints,
                vec![key.fingerprint],
                requires_unlock,
            );
        }
    }

    fingerprints
}

fn available_tool_keys(
    list_keys: &super::ListEntryToolKeys,
) -> Result<Vec<EntryToolKeySummary>, String> {
    let mut unique = Vec::new();
    for key in list_keys()? {
        push_unique_available_tool_key(&mut unique, key.fingerprint, key.user_ids);
    }
    Ok(unique)
}

fn push_unique_available_tool_key(
    keys: &mut Vec<EntryToolKeySummary>,
    fingerprint: String,
    user_ids: Vec<String>,
) {
    if keys
        .iter()
        .any(|existing| existing.fingerprint.eq_ignore_ascii_case(&fingerprint))
    {
        return;
    }

    keys.push(EntryToolKeySummary {
        fingerprint,
        user_ids,
    });
}

fn collect_tool_standard_recipients(
    requests: &[FieldValueRequest],
    relevant_scopes: &super::RelevantStoreScopes,
    read_recipients: &super::ReadStoreRecipients,
    root_scope: &str,
) -> Vec<String> {
    let mut recipients = Vec::new();
    for store_root in tool_request_store_roots(requests) {
        for scope in tool_request_store_scopes(&store_root, relevant_scopes, root_scope) {
            for recipient in read_recipients(store_root.clone(), scope) {
                push_unique_standard_tool_recipient(&mut recipients, recipient);
            }
        }
    }
    recipients
}

fn push_unique_standard_tool_recipient(recipients: &mut Vec<String>, candidate: String) {
    let candidate = candidate.trim();
    if candidate.is_empty()
        || recipients
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(candidate))
    {
        return;
    }

    recipients.push(candidate.to_string());
}

fn tool_request_store_roots(requests: &[FieldValueRequest]) -> Vec<String> {
    let mut store_roots = Vec::new();
    for request in requests {
        if !store_roots.iter().any(|existing| existing == &request.root) {
            store_roots.push(request.root.clone());
        }
    }
    store_roots
}

fn tool_request_store_scopes(
    store_root: &str,
    relevant_scopes: &super::RelevantStoreScopes,
    root_scope: &str,
) -> Vec<String> {
    let mut scopes = vec![root_scope.to_string()];
    for scope in relevant_scopes(store_root.to_string()) {
        if !scopes.iter().any(|existing| existing == &scope) {
            scopes.push(scope);
        }
    }
    scopes
}

fn tool_recipient_matches_key(recipient: &str, key: &EntryToolKeySummary) -> bool {
    let recipient = recipient.trim();
    recipient.eq_ignore_ascii_case(&key.fingerprint)
        || key
            .user_ids
            .iter()
            .any(|user_id| user_id.eq_ignore_ascii_case(recipient))
}

fn append_unlockable_tool_fingerprints(
    fingerprints: &mut Vec<String>,
    candidates: Vec<String>,
    requires_unlock: &EntryToolKeyRequiresUnlock,
) {
    for candidate in candidates {
        if !matches!(requires_unlock(candidate.clone()), Ok(true)) {
            continue;
        }

        if !fingerprints
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&candidate))
        {
            fingerprints.push(candidate);
        }
    }
}

fn prompt_tool_unlock_sequence(
    overlay: &ToastOverlay,
    fingerprints: Vec<String>,
    prompt_unlock: PromptEntryUnlock,
    on_finish: Rc<dyn Fn(bool)>,
) {
    if fingerprints.is_empty() {
        on_finish(true);
        return;
    }

    prompt_tool_unlock_at_index(
        overlay.clone(),
        Rc::new(fingerprints),
        0,
        prompt_unlock,
        on_finish,
    );
}

fn prompt_tool_unlock_at_index(
    overlay: ToastOverlay,
    fingerprints: Rc<Vec<String>>,
    index: usize,
    prompt_unlock: PromptEntryUnlock,
    on_finish: Rc<dyn Fn(bool)>,
) {
    let Some(fingerprint) = fingerprints.get(index).cloned() else {
        on_finish(true);
        return;
    };

    let overlay_for_next = overlay.clone();
    let fingerprints_for_next = fingerprints.clone();
    let on_finish_for_next = on_finish.clone();
    let on_finish_for_result = on_finish.clone();
    let prompt_unlock_for_next = prompt_unlock.clone();
    prompt_unlock(
        &overlay,
        fingerprint,
        Rc::new(move || {
            prompt_tool_unlock_at_index(
                overlay_for_next.clone(),
                fingerprints_for_next.clone(),
                index + 1,
                prompt_unlock_for_next.clone(),
                on_finish_for_next.clone(),
            );
        }),
        Rc::new(move |success| {
            if !success {
                on_finish_for_result(false);
            }
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::push_unique_standard_tool_recipient;

    #[test]
    fn standard_tool_recipients_are_case_insensitive_and_unique() {
        let mut recipients = vec!["ABCDEF0123456789".to_string()];
        push_unique_standard_tool_recipient(&mut recipients, "abcdef0123456789".to_string());
        push_unique_standard_tool_recipient(&mut recipients, "FEDCBA9876543210".to_string());

        assert_eq!(
            recipients,
            vec![
                "ABCDEF0123456789".to_string(),
                "FEDCBA9876543210".to_string(),
            ]
        );
    }
}
