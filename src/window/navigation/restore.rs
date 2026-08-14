//! Root route ordering for subject-owned navigation presentations.

use super::state::WindowNavigationState;
use crate::window::tool_hub::tool_hub_page_presentation;
use adw::prelude::*;
use keycord_docs::{documentation_detail_presentation, documentation_index_presentation};
use keycord_entries::ui::page::{entry_page_navigation_routes, PasswordPageState};
use keycord_entries::ui::tools::entry_tool_navigation_routes;
use keycord_git::ui::{git_audit_page_presentation, sync_store_git_page_header, StoreGitPageState};
use keycord_keys::ui::key_generation_navigation_routes;
use keycord_preferences::ui::preferences_page_presentation;
use keycord_shell::log_page_presentation;
use keycord_shell::navigation::{
    restore_navigation_for_current_page, HasWindowChrome, NavigationPageId, NavigationPageRoute,
};
use keycord_stores::ui::management::{sync_store_recipients_page_header, StoreRecipientsPageState};
use keycord_stores::ui::store_import_page_presentation;

const STORE_IMPORT_PAGE: NavigationPageId = NavigationPageId::new("store-import");
const SETTINGS_PAGE: NavigationPageId = NavigationPageId::new("settings");
const TOOLS_PAGE: NavigationPageId = NavigationPageId::new("tools");
const DOCUMENTATION_PAGE: NavigationPageId = NavigationPageId::new("documentation");
const DOCUMENTATION_DETAIL_PAGE: NavigationPageId = NavigationPageId::new("documentation-detail");
const TOOL_AUDIT_PAGE: NavigationPageId = NavigationPageId::new("tool-audit");
const STORE_RECIPIENTS_PAGE: NavigationPageId = NavigationPageId::new("store-recipients");
const STORE_GIT_PAGE: NavigationPageId = NavigationPageId::new("store-git");
const LOG_PAGE: NavigationPageId = NavigationPageId::new("log");

pub fn restore_window_for_current_page(
    state: &WindowNavigationState,
    password_page: &PasswordPageState,
    recipients_page: &StoreRecipientsPageState,
    store_git_page: &StoreGitPageState,
) -> bool {
    let chrome = state.window_chrome();
    let mut before_root = vec![NavigationPageRoute::secondary(
        STORE_IMPORT_PAGE,
        &state.store_import_page,
        store_import_page_presentation(),
    )];
    before_root.extend(key_generation_navigation_routes(&state.keys));

    let recipients_page_for_restore = recipients_page.clone();
    let save_for_recipients = state.save.clone();
    let store_git_page_for_restore = store_git_page.clone();

    let mut routes = Vec::new();
    routes.extend(entry_page_navigation_routes(password_page));
    routes.push(NavigationPageRoute::secondary(
        SETTINGS_PAGE,
        &state.settings_page,
        preferences_page_presentation(),
    ));
    routes.push(NavigationPageRoute::secondary(
        TOOLS_PAGE,
        &state.tools_page,
        tool_hub_page_presentation(),
    ));
    routes.push(NavigationPageRoute::secondary(
        DOCUMENTATION_PAGE,
        &state.docs_page,
        documentation_index_presentation(),
    ));
    routes.push(NavigationPageRoute::secondary(
        DOCUMENTATION_DETAIL_PAGE,
        &state.docs_detail_page,
        documentation_detail_presentation(state.docs_detail_page.title()),
    ));
    routes.extend(entry_tool_navigation_routes(&state.entries));
    routes.push(NavigationPageRoute::secondary(
        TOOL_AUDIT_PAGE,
        &state.tools_audit_page,
        git_audit_page_presentation(),
    ));
    routes.push(NavigationPageRoute::callback(
        STORE_RECIPIENTS_PAGE,
        &recipients_page.page,
        move || {
            keycord_entries::ui::actions::configure_password_save_button(&save_for_recipients);
            sync_store_recipients_page_header(&recipients_page_for_restore);
        },
    ));
    routes.push(NavigationPageRoute::callback(
        STORE_GIT_PAGE,
        &store_git_page.page,
        move || sync_store_git_page_header(&store_git_page_for_restore),
    ));
    routes.push(NavigationPageRoute::secondary(
        LOG_PAGE,
        &state.log_page,
        log_page_presentation(),
    ));

    let restore_root = crate::composition::navigation::root_page_chrome_callback();
    restore_navigation_for_current_page(&state.nav, &chrome, &restore_root, &before_root, &routes)
        .is_root()
}

#[cfg(test)]
mod tests {
    use super::{
        DOCUMENTATION_DETAIL_PAGE, DOCUMENTATION_PAGE, LOG_PAGE, SETTINGS_PAGE, STORE_GIT_PAGE,
        STORE_IMPORT_PAGE, STORE_RECIPIENTS_PAGE, TOOLS_PAGE, TOOL_AUDIT_PAGE,
    };
    use keycord_entries::ui::page::{PASSWORD_PAGE_ID, RAW_TEXT_PAGE_ID};
    use keycord_entries::ui::tools::{
        FIELD_VALUES_PAGE_ID, VALUE_VALUES_PAGE_ID, WEAK_PASSWORDS_PAGE_ID,
    };
    use keycord_keys::ui::{HARDWARE_KEY_GENERATION_PAGE_ID, PRIVATE_KEY_GENERATION_PAGE_ID};

    #[test]
    fn composed_navigation_page_ids_are_stable() {
        assert_eq!(
            [
                STORE_IMPORT_PAGE,
                PRIVATE_KEY_GENERATION_PAGE_ID,
                HARDWARE_KEY_GENERATION_PAGE_ID,
                PASSWORD_PAGE_ID,
                RAW_TEXT_PAGE_ID,
                SETTINGS_PAGE,
                TOOLS_PAGE,
                DOCUMENTATION_PAGE,
                DOCUMENTATION_DETAIL_PAGE,
                FIELD_VALUES_PAGE_ID,
                VALUE_VALUES_PAGE_ID,
                WEAK_PASSWORDS_PAGE_ID,
                TOOL_AUDIT_PAGE,
                STORE_RECIPIENTS_PAGE,
                STORE_GIT_PAGE,
                LOG_PAGE,
            ]
            .map(|page| page.as_str()),
            [
                "store-import",
                "private-key-generation",
                "hardware-key-generation",
                "password",
                "raw-text",
                "settings",
                "tools",
                "documentation",
                "documentation-detail",
                "tool-field-values",
                "tool-value-values",
                "tool-weak-passwords",
                "tool-audit",
                "store-recipients",
                "store-git",
                "log",
            ]
        );
    }
}
