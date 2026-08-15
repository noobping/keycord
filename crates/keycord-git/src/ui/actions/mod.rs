//! Application-level Git actions and their composition ports.

#[cfg(not(target_os = "linux"))]
mod disabled;
#[cfg(target_os = "linux")]
mod enabled;

#[cfg(not(target_os = "linux"))]
use self::disabled as imp;
#[cfg(target_os = "linux")]
use self::enabled as imp;

use adw::gtk::Button;
use adw::{
    ApplicationWindow, NavigationPage, NavigationView, StatusPage, ToastOverlay, WindowTitle,
};
use keycord_shell::actions::set_window_action_enabled;
use keycord_shell::navigation::{HasWindowChrome, WindowChrome, WindowPageState};
use keycord_stores::ui::management::NUMBERED_STORE_SHORTCUT_COUNT;
use keycord_stores::ui::recipient_page::StoreRecipientsPageState;
use std::rc::Rc;

use super::store_page::StoreGitPageState;
use super::window_widgets::GitWindowWidgets;

pub type PromptStoreClone =
    Rc<dyn Fn(&ApplicationWindow, &ToastOverlay, Rc<dyn Fn(String, String)>)>;

#[derive(Clone)]
pub struct GitActionPorts {
    pub prompt_store_clone: PromptStoreClone,
    pub configured_stores: Rc<dyn Fn() -> Vec<String>>,
    pub set_configured_stores: Rc<dyn Fn(Vec<String>) -> Result<(), String>>,
    pub refresh_after_operation: Rc<dyn Fn()>,
    pub restore_navigation: Rc<dyn Fn()>,
    pub set_application_busy: Rc<dyn Fn(bool)>,
}

// Non-Linux builds register inert Git actions and only read the window field.
#[derive(Clone)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct GitActionState {
    window: ApplicationWindow,
    overlay: ToastOverlay,
    nav: NavigationView,
    back: Button,
    add: Button,
    find: Button,
    primary_action: Button,
    secondary_action: Button,
    save: Button,
    raw: Button,
    title: WindowTitle,
    recipients_page: StoreRecipientsPageState,
    store_git_page: StoreGitPageState,
    busy_page: NavigationPage,
    busy_status: StatusPage,
    ports: GitActionPorts,
}

impl GitActionState {
    /// Build Git action state from its owner bundle and application chrome/ports.
    pub fn new(
        widgets: &GitWindowWidgets,
        page_state: WindowPageState,
        overlay: &ToastOverlay,
        recipients_page: &StoreRecipientsPageState,
        store_git_page: &StoreGitPageState,
        ports: GitActionPorts,
    ) -> Self {
        Self {
            window: page_state.window,
            overlay: overlay.clone(),
            nav: page_state.nav,
            back: page_state.back,
            add: page_state.add,
            find: page_state.find,
            primary_action: page_state.primary_action,
            secondary_action: page_state.secondary_action,
            save: page_state.save,
            raw: page_state.raw,
            title: page_state.title,
            recipients_page: recipients_page.clone(),
            store_git_page: store_git_page.clone(),
            busy_page: page_state.page,
            busy_status: widgets.git_busy_status.clone(),
            ports,
        }
    }
}

impl HasWindowChrome for GitActionState {
    fn window_chrome(&self) -> WindowChrome<'_> {
        WindowChrome {
            back: &self.back,
            add: &self.add,
            find: &self.find,
            primary_action: &self.primary_action,
            secondary_action: &self.secondary_action,
            save: &self.save,
            raw: &self.raw,
            title: &self.title,
        }
    }
}

pub fn set_git_action_availability(window: &ApplicationWindow, enabled: bool) {
    for action in ["git-clone", "open-git", "synchronize"] {
        let _ = set_window_action_enabled(window, action, enabled);
    }
}

fn git_owned_busy_action_names() -> Vec<String> {
    let mut actions = ["git-clone", "open-git", "synchronize"]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    for slot in 1..=NUMBERED_STORE_SHORTCUT_COUNT {
        actions.push(format!("open-store-git-{slot}"));
    }
    actions
}

pub(super) fn set_git_operation_busy(
    window: &ApplicationWindow,
    set_application_busy: &dyn Fn(bool),
    busy: bool,
) {
    for action in git_owned_busy_action_names() {
        let _ = set_window_action_enabled(window, &action, !busy);
    }
    set_application_busy(busy);
}

pub use self::imp::{
    clone_store_repository, handle_git_busy_back, register_open_git_action,
    register_synchronize_action,
};

#[cfg(test)]
mod tests {
    use super::git_owned_busy_action_names;

    #[test]
    fn busy_action_policy_contains_only_git_owned_actions() {
        let actions = git_owned_busy_action_names();

        assert!(actions.iter().any(|action| action == "git-clone"));
        assert!(actions.iter().any(|action| action == "open-git"));
        assert!(actions.iter().any(|action| action == "synchronize"));
        assert!(actions.iter().any(|action| action == "open-store-git-1"));
        assert_eq!(
            actions.len(),
            3 + keycord_stores::ui::management::NUMBERED_STORE_SHORTCUT_COUNT
        );
        assert!(actions.iter().all(|action| matches!(
            action.as_str(),
            "git-clone" | "open-git" | "synchronize"
        ) || action.starts_with("open-store-git-")));
    }
}
