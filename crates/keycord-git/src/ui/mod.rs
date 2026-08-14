//! Git-specific GTK presentation and composition ports.

mod actions;
mod audit;
mod focus;
mod remote;
mod shortcuts;
mod signing;
mod store_page;
mod window_widgets;

pub use actions::{
    clone_store_repository, handle_git_busy_back, register_open_git_action,
    register_synchronize_action, set_git_action_availability, GitActionPorts, GitActionState,
};
pub use audit::{
    audit_tool_cache_should_clear, git_audit_page_presentation, GitAuditPagePorts,
    GitAuditPageState, GitAuditPageWidgets, GitAuditPreferencesPorts, GitAuditWindowNavigation,
};
pub use focus::{
    connect_git_page_keyboard_navigation, focus_first_visible_git_page_target,
    visible_git_page_contains_focus,
};
pub use remote::{present_remote_dialog, RemoteDialogRequest};
pub use shortcuts::configure_git_shortcuts;
pub use signing::{
    integrated_git_ports, prompt_private_key_unlock_for_entry_git_commit_if_needed,
    prompt_private_key_unlock_for_store_git_commit_if_needed, GitSigningPrivateKey,
    GitSigningUiPorts,
};
pub use store_page::{
    connect_store_git_controls, present_store_git_dialog, rebuild_store_git_page,
    rebuild_store_recipients_git_row, show_store_git_page, show_store_git_page_from_recipients,
    sync_store_git_page_header, StoreGitPagePorts, StoreGitPageState,
};
pub use window_widgets::GitWindowWidgets;
