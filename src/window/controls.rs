use crate::window::navigation::{restore_window_for_current_page, WindowNavigationState};
use adw::prelude::*;
use adw::{Application, ApplicationWindow, NavigationPage};
use keycord_docs::configure_documentation_shortcuts;
use keycord_entries::ui::actions::configure_entry_window_shortcuts;
use keycord_entries::ui::list::PasswordListVisibilityState;
use keycord_entries::ui::page::{
    retry_open_password_entry_if_needed, show_password_list_page, PasswordPageState,
};
use keycord_git::ui::{
    configure_git_shortcuts, handle_git_busy_back, GitActionState, StoreGitPageState,
};
use keycord_preferences::ui::configure_preferences_shortcuts;
use keycord_runtime::capabilities::has_host_permission;
use keycord_shell::actions::{activate_widget_action, register_window_action};
use keycord_shell::configure_shell_shortcuts;
use keycord_shell::ui::{navigation_stack_is_root, visible_navigation_page_is};
use keycord_stores::ui::configure_store_shortcuts;
use keycord_stores::ui::management::StoreRecipientsPageState;
use keycord_stores::ui::recipient_page::handle_store_recipients_subpage_back;
use std::rc::Rc;

#[derive(Clone)]
pub struct PlatformBackActionState {
    pub git_actions: GitActionState,
}

fn before_back_action(state: &PlatformBackActionState) -> bool {
    handle_git_busy_back(&state.git_actions)
}

#[derive(Clone)]
pub struct BackActionState {
    pub password_page: PasswordPageState,
    pub recipients_page: StoreRecipientsPageState,
    pub store_git_page: StoreGitPageState,
    pub navigation: WindowNavigationState,
    pub visibility: PasswordListVisibilityState,
    pub platform: PlatformBackActionState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextSaveTarget {
    Password,
    StoreRecipients,
    Synchronize,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContextReloadTarget {
    PasswordList,
    StoreRecipients,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VisibleContextPage {
    Root,
    Password,
    StoreRecipients,
    Other,
}

fn visible_context_page(
    navigation: &WindowNavigationState,
    recipients_page: &NavigationPage,
) -> VisibleContextPage {
    if visible_navigation_page_is(&navigation.nav, &navigation.entries.password_page)
        || visible_navigation_page_is(&navigation.nav, &navigation.entries.raw_text_page)
    {
        VisibleContextPage::Password
    } else if visible_navigation_page_is(&navigation.nav, recipients_page) {
        VisibleContextPage::StoreRecipients
    } else if navigation_stack_is_root(&navigation.nav) {
        VisibleContextPage::Root
    } else {
        VisibleContextPage::Other
    }
}

const fn context_save_target_from_page(
    page: VisibleContextPage,
    has_host_permission: bool,
) -> ContextSaveTarget {
    match page {
        VisibleContextPage::Password => ContextSaveTarget::Password,
        VisibleContextPage::StoreRecipients => ContextSaveTarget::StoreRecipients,
        VisibleContextPage::Root if has_host_permission => ContextSaveTarget::Synchronize,
        VisibleContextPage::Root | VisibleContextPage::Other => ContextSaveTarget::None,
    }
}

fn context_save_target(
    navigation: &WindowNavigationState,
    recipients_page: &NavigationPage,
) -> ContextSaveTarget {
    context_save_target_from_page(
        visible_context_page(navigation, recipients_page),
        has_host_permission(),
    )
}

const fn context_reload_target_from_page(page: VisibleContextPage) -> ContextReloadTarget {
    match page {
        VisibleContextPage::Root => ContextReloadTarget::PasswordList,
        VisibleContextPage::StoreRecipients => ContextReloadTarget::StoreRecipients,
        VisibleContextPage::Password | VisibleContextPage::Other => ContextReloadTarget::None,
    }
}

fn context_reload_target(
    navigation: &WindowNavigationState,
    recipients_page: &NavigationPage,
) -> ContextReloadTarget {
    context_reload_target_from_page(visible_context_page(navigation, recipients_page))
}

pub fn register_context_save_action(
    window: &ApplicationWindow,
    navigation: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) {
    let action_window = window.clone();
    let dispatch_window = action_window.clone();
    let navigation = navigation.clone();
    let recipients_page = recipients_page.page.clone();
    register_window_action(
        &action_window,
        "context-save",
        move || match context_save_target(&navigation, &recipients_page) {
            ContextSaveTarget::Password => {
                activate_widget_action(&dispatch_window, "win.save-password");
            }
            ContextSaveTarget::StoreRecipients => {
                activate_widget_action(&dispatch_window, "win.save-store-recipients");
            }
            ContextSaveTarget::Synchronize => {
                activate_widget_action(&dispatch_window, "win.synchronize");
            }
            ContextSaveTarget::None => {}
        },
    );
}

pub fn register_context_reload_action(
    window: &ApplicationWindow,
    navigation: &WindowNavigationState,
    recipients_page: &StoreRecipientsPageState,
) {
    let action_window = window.clone();
    let dispatch_window = action_window.clone();
    let navigation = navigation.clone();
    let recipients_page = recipients_page.page.clone();
    register_window_action(
        &action_window,
        "context-reload",
        move || match context_reload_target(&navigation, &recipients_page) {
            ContextReloadTarget::PasswordList => {
                activate_widget_action(&dispatch_window, "win.reload-password-list");
            }
            ContextReloadTarget::StoreRecipients => {
                activate_widget_action(&dispatch_window, "win.reload-store-recipients-list");
            }
            ContextReloadTarget::None => {}
        },
    );
}

pub fn register_context_undo_action(window: &ApplicationWindow, undo: Rc<dyn Fn()>) {
    register_window_action(window, "context-undo", move || undo());
}

pub type ToggleFindCallback = Rc<dyn Fn() -> bool>;

pub fn register_toggle_find_action(
    window: &adw::ApplicationWindow,
    callbacks: Vec<ToggleFindCallback>,
) {
    register_window_action(window, "toggle-find", move || {
        let _ = callbacks.iter().any(|toggle_find| toggle_find());
    });
}

pub fn register_back_action(window: &adw::ApplicationWindow, state: &BackActionState) {
    let state = state.clone();
    register_window_action(window, "back", move || {
        if before_back_action(&state.platform) {
            return;
        }
        if handle_store_recipients_subpage_back(&state.recipients_page) {
            return;
        }

        state.navigation.nav.pop();
        if restore_window_for_current_page(
            &state.navigation,
            &state.password_page,
            &state.recipients_page,
            &state.store_git_page,
        ) {
            show_password_list_page(
                &state.password_page,
                state.visibility.show_hidden(),
                state.visibility.show_duplicates(),
            );
            return;
        }

        let _ = retry_open_password_entry_if_needed(&state.password_page);
    });
}

pub fn register_go_home_action(window: &adw::ApplicationWindow, state: &BackActionState) {
    let state = state.clone();
    register_window_action(window, "go-home", move || {
        show_password_list_page(
            &state.password_page,
            state.visibility.show_hidden(),
            state.visibility.show_duplicates(),
        );
    });
}

pub fn configure_window_shortcuts(app: &Application) {
    configure_shell_shortcuts(app);
    configure_entry_window_shortcuts(app);
    configure_store_shortcuts(app);
    configure_preferences_shortcuts(app);
    configure_git_shortcuts(app);
    configure_documentation_shortcuts(app);

    app.set_accels_for_action("win.context-save", &["<primary>s"]);
    app.set_accels_for_action("win.context-reload", &["F5"]);
    app.set_accels_for_action("win.context-undo", &["<primary>z"]);
    app.set_accels_for_action("win.toggle-find", &["<primary>f"]);
    app.set_accels_for_action("win.open-tools", &["<primary>t"]);
}

#[cfg(test)]
mod tests {
    use super::{
        context_reload_target_from_page, context_save_target_from_page, ContextReloadTarget,
        ContextSaveTarget, VisibleContextPage,
    };

    #[test]
    fn context_save_prefers_password_pages() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Password, true),
            ContextSaveTarget::Password
        );
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Password, true),
            ContextSaveTarget::Password
        );
    }

    #[test]
    fn context_save_uses_recipients_page_before_list_mode() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::StoreRecipients, true),
            ContextSaveTarget::StoreRecipients
        );
    }

    #[test]
    fn context_save_uses_sync_on_the_root_list_page() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Root, true),
            ContextSaveTarget::Synchronize
        );
    }

    #[test]
    fn context_save_skips_sync_when_git_is_unavailable() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Root, false),
            ContextSaveTarget::None
        );
    }

    #[test]
    fn context_save_is_disabled_on_other_secondary_pages() {
        assert_eq!(
            context_save_target_from_page(VisibleContextPage::Other, true),
            ContextSaveTarget::None
        );
    }

    #[test]
    fn context_reload_uses_the_root_password_list() {
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::Root),
            ContextReloadTarget::PasswordList
        );
    }

    #[test]
    fn context_reload_uses_the_recipients_page_list() {
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::StoreRecipients),
            ContextReloadTarget::StoreRecipients
        );
    }

    #[test]
    fn context_reload_is_disabled_on_editor_and_other_pages() {
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::Password),
            ContextReloadTarget::None
        );
        assert_eq!(
            context_reload_target_from_page(VisibleContextPage::Other),
            ContextReloadTarget::None
        );
    }
}
