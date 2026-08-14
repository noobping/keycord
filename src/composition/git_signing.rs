//! Connects Git signing prompts to the selected Keys backend.

use crate::composition::backend::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients,
};
use adw::ToastOverlay;
use keycord_git::ui::GitSigningUiPorts;
use keycord_stores::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
use std::rc::Rc;

fn git_signing_ports() -> GitSigningUiPorts {
    GitSigningUiPorts::new(
        git_commit_private_key_requiring_unlock_for_entry,
        git_commit_private_key_requiring_unlock_for_store_recipients,
    )
}

pub fn prompt_private_key_unlock_for_entry_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    label: &str,
    after_unlock: &Rc<dyn Fn()>,
) -> bool {
    keycord_git::ui::prompt_private_key_unlock_for_entry_git_commit_if_needed(
        overlay,
        store_root,
        label,
        &git_signing_ports(),
        after_unlock,
    )
}

pub fn prompt_private_key_unlock_for_store_git_commit_if_needed(
    overlay: &ToastOverlay,
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    after_unlock: &Rc<dyn Fn()>,
) -> bool {
    keycord_git::ui::prompt_private_key_unlock_for_store_git_commit_if_needed(
        overlay,
        store_root,
        recipients,
        private_key_requirement,
        &git_signing_ports(),
        after_unlock,
    )
}
