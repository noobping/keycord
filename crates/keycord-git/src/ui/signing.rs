use adw::{ApplicationWindow, Toast, ToastOverlay};
use keycord_keys::{
    borrow_unlocked_hardware_private_key, borrow_unlocked_ripasso_private_key,
    list_connected_smartcard_keys, list_ripasso_private_keys,
    ripasso_private_key_requires_session_unlock, ripasso_private_key_title,
    sign_with_hardware_session, unlock_ripasso_private_key_for_session, PrivateKeyUnlockKind,
    PrivateKeyUnlockRequest,
};
use keycord_runtime::{i18n::gettext, log_error, log_info};
use keycord_shell::background::spawn_result_task_with_finalizer;
use keycord_shell::ui::application_window_for_widget;
use keycord_stores::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitSigningPrivateKey {
    pub fingerprint: String,
    pub unlock_kind: PrivateKeyUnlockKind,
}

pub type ResolveEntryCommitKey = Rc<dyn Fn(&str, &str) -> Result<Option<String>, String>>;
pub type ResolveStoreCommitKey = Rc<
    dyn Fn(
        &str,
        &StoreRecipients,
        StoreRecipientsPrivateKeyRequirement,
    ) -> Result<Option<String>, String>,
>;
pub type PresentGitUnlockDialog = Rc<
    dyn Fn(
        &ApplicationWindow,
        &ToastOverlay,
        &str,
        Option<String>,
        PrivateKeyUnlockKind,
        Rc<dyn Fn(PrivateKeyUnlockRequest)>,
        Rc<dyn Fn()>,
    ),
>;
pub type PrivateKeyTitle = Rc<dyn Fn(&str) -> Result<String, String>>;

#[derive(Clone)]
pub struct GitSigningUiPorts {
    pub resolve_entry_commit_key: ResolveEntryCommitKey,
    pub resolve_store_commit_key: ResolveStoreCommitKey,
    pub list_private_keys: Rc<dyn Fn() -> Result<Vec<GitSigningPrivateKey>, String>>,
    pub connected_smartcard_fingerprints: Rc<dyn Fn() -> Result<Vec<String>, String>>,
    pub private_key_title: PrivateKeyTitle,
    pub unlock_for_session:
        Arc<dyn Fn(String, PrivateKeyUnlockRequest) -> Result<(), String> + Send + Sync>,
    pub present_unlock_dialog: PresentGitUnlockDialog,
}

impl GitSigningUiPorts {
    pub fn new(
        resolve_entry_commit_key: impl Fn(&str, &str) -> Result<Option<String>, String> + 'static,
        resolve_store_commit_key: impl Fn(
                &str,
                &StoreRecipients,
                StoreRecipientsPrivateKeyRequirement,
            ) -> Result<Option<String>, String>
            + 'static,
    ) -> Self {
        Self {
            resolve_entry_commit_key: Rc::new(resolve_entry_commit_key),
            resolve_store_commit_key: Rc::new(resolve_store_commit_key),
            list_private_keys: Rc::new(|| {
                list_ripasso_private_keys().map(|keys| {
                    keys.into_iter()
                        .map(|key| GitSigningPrivateKey {
                            fingerprint: key.fingerprint,
                            unlock_kind: key.protection.into(),
                        })
                        .collect()
                })
            }),
            connected_smartcard_fingerprints: Rc::new(|| {
                list_connected_smartcard_keys()
                    .map(|keys| keys.into_iter().map(|key| key.fingerprint).collect())
            }),
            private_key_title: Rc::new(ripasso_private_key_title),
            unlock_for_session: Arc::new(|fingerprint, request| {
                unlock_ripasso_private_key_for_session(&fingerprint, request)
                    .map(|_| ())
                    .map_err(|err| err.to_string())
            }),
            present_unlock_dialog: Rc::new(
                |window, overlay, title, subtitle, kind, on_submit, on_close| {
                    keycord_keys::ui::present_private_key_unlock_dialog_with_close_handler(
                        window,
                        overlay,
                        title,
                        subtitle.as_deref(),
                        kind,
                        move |request| on_submit(request),
                        move || on_close(),
                    );
                },
            ),
        }
    }
}

fn list_integrated_git_private_keys() -> Result<Vec<crate::GitPrivateKey>, String> {
    list_ripasso_private_keys().map(|keys| {
        keys.into_iter()
            .map(|key| crate::GitPrivateKey {
                fingerprint: key.fingerprint,
                user_ids: key.user_ids,
            })
            .collect()
    })
}

fn unlocked_signing_cert(fingerprint: &str) -> Result<Option<Arc<sequoia_openpgp::Cert>>, String> {
    borrow_unlocked_ripasso_private_key(fingerprint)
}

fn sign_with_unlocked_hardware(
    fingerprint: &str,
    contents: &str,
) -> Result<Option<String>, String> {
    let Some(session) = borrow_unlocked_hardware_private_key(fingerprint)? else {
        return Ok(None);
    };
    sign_with_hardware_session(&session, contents)
        .map(Some)
        .map_err(|err| err.to_string())
}

pub fn integrated_git_ports() -> crate::IntegratedGitPorts {
    crate::IntegratedGitPorts {
        list_private_keys: list_integrated_git_private_keys,
        private_key_requires_session_unlock: ripasso_private_key_requires_session_unlock,
        unlocked_signing_cert,
        sign_with_unlocked_hardware,
    }
}

fn continue_without_git_signature(overlay: &ToastOverlay, reason: &str, action: &Rc<dyn Fn()>) {
    log_info(reason.to_string());
    overlay.add_toast(Toast::new(&gettext("Saving without a Git signature.")));
    action();
}

fn private_key_unlock_kind(ports: &GitSigningUiPorts, fingerprint: &str) -> PrivateKeyUnlockKind {
    match (ports.list_private_keys)() {
        Ok(keys) => {
            if let Some(kind) = keys
                .into_iter()
                .find(|key| key.fingerprint.eq_ignore_ascii_case(fingerprint))
                .map(|key| key.unlock_kind)
            {
                return kind;
            }
        }
        Err(err) => {
            log_error(format!(
                "Failed to read private key protection for '{fingerprint}': {err}"
            ));
        }
    }

    match (ports.connected_smartcard_fingerprints)() {
        Ok(keys) => keys
            .into_iter()
            .find(|key| key.eq_ignore_ascii_case(fingerprint))
            .map(|_| PrivateKeyUnlockKind::HardwareOpenPgpCard)
            .unwrap_or(PrivateKeyUnlockKind::Password),
        Err(err) => {
            log_error(format!(
                "Failed to inspect connected smartcards for '{fingerprint}': {err}"
            ));
            PrivateKeyUnlockKind::Password
        }
    }
}

fn start_private_key_unlock_for_git_commit(
    overlay: &ToastOverlay,
    fingerprint: String,
    request: PrivateKeyUnlockRequest,
    ports: &GitSigningUiPorts,
    after_unlock_attempt: &Rc<dyn Fn()>,
) {
    let overlay = overlay.clone();
    let overlay_for_disconnect = overlay.clone();
    let fingerprint_for_worker = fingerprint.clone();
    let fingerprint_for_failure = fingerprint.clone();
    let after_unlock_attempt_for_result = after_unlock_attempt.clone();
    let after_unlock_attempt_for_disconnect = after_unlock_attempt.clone();
    let unlock_for_session = ports.unlock_for_session.clone();
    spawn_result_task_with_finalizer(
        move || unlock_for_session(fingerprint_for_worker, request),
        || {},
        move |result| match result {
            Ok(()) => {
                after_unlock_attempt_for_result();
            }
            Err(err) => {
                log_error(format!("Failed to unlock ripasso private key: {err}"));
                continue_without_git_signature(
                    &overlay,
                    &format!(
                        "Couldn't unlock private key {fingerprint_for_failure} for Git signing. Continuing without a signature."
                    ),
                    &after_unlock_attempt_for_result,
                );
            }
        },
        move || {
            log_error("Private key unlock worker disconnected unexpectedly.".to_string());
            continue_without_git_signature(
                &overlay_for_disconnect,
                &format!(
                    "Private key unlock worker disconnected while preparing a Git signature for {fingerprint}."
                ),
                &after_unlock_attempt_for_disconnect,
            );
        },
    );
}

fn prompt_private_key_unlock_for_git_commit_if_needed(
    overlay: &ToastOverlay,
    fingerprint: Result<Option<String>, String>,
    context: &str,
    ports: &GitSigningUiPorts,
    after_unlock_attempt: &Rc<dyn Fn()>,
) -> bool {
    let context = context.to_string();

    match fingerprint {
        Ok(Some(fingerprint)) => {
            let Some(window) = application_window_for_widget(overlay) else {
                log_error(
                    "Couldn't find the application window for the Git signing unlock dialog."
                        .to_string(),
                );
                continue_without_git_signature(
                    overlay,
                    "Couldn't present the Git signing unlock dialog. Continuing without a signature.",
                    after_unlock_attempt,
                );
                return true;
            };
            let key_title = match (ports.private_key_title)(&fingerprint) {
                Ok(title) => Some(title),
                Err(err) => {
                    log_error(format!(
                        "Failed to read private key title for '{fingerprint}': {err}"
                    ));
                    None
                }
            };
            let overlay_for_submit = overlay.clone();
            let kind = private_key_unlock_kind(ports, &fingerprint);
            let fingerprint_for_submit = fingerprint;
            let after_unlock_attempt_for_submit = after_unlock_attempt.clone();
            let overlay_for_close = overlay.clone();
            let after_unlock_attempt_for_close = after_unlock_attempt.clone();
            let context_for_close = context;
            let ports_for_submit = ports.clone();
            let on_submit: Rc<dyn Fn(PrivateKeyUnlockRequest)> = Rc::new(move |request| {
                start_private_key_unlock_for_git_commit(
                    &overlay_for_submit,
                    fingerprint_for_submit.clone(),
                    request,
                    &ports_for_submit,
                    &after_unlock_attempt_for_submit,
                );
            });
            let on_close: Rc<dyn Fn()> = Rc::new(move || {
                continue_without_git_signature(
                    &overlay_for_close,
                    &format!(
                        "Dismissed the Git signing unlock prompt for {context_for_close}. Continuing without a signature."
                    ),
                    &after_unlock_attempt_for_close,
                );
            });
            (ports.present_unlock_dialog)(
                &window,
                overlay,
                "Unlock key",
                key_title,
                kind,
                on_submit,
                on_close,
            );
            true
        }
        Ok(None) => false,
        Err(err) => {
            log_error(format!(
                "Failed to resolve the private key needed to sign the Git commit for {context}: {err}"
            ));
            false
        }
    }
}

pub fn prompt_private_key_unlock_for_entry_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    label: &str,
    ports: &GitSigningUiPorts,
    after_unlock: &Rc<dyn Fn()>,
) -> bool {
    prompt_private_key_unlock_for_git_commit_if_needed(
        overlay,
        (ports.resolve_entry_commit_key)(store_root, label),
        label,
        ports,
        after_unlock,
    )
}

pub fn prompt_private_key_unlock_for_store_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    ports: &GitSigningUiPorts,
    after_unlock: &Rc<dyn Fn()>,
) -> bool {
    prompt_private_key_unlock_for_git_commit_if_needed(
        overlay,
        (ports.resolve_store_commit_key)(store_root, recipients, private_key_requirement),
        store_root,
        ports,
        after_unlock,
    )
}
