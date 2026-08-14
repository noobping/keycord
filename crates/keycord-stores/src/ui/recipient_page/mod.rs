use crate::recipients::{
    read_store_private_key_requirement, read_store_private_key_requirement_for_scope,
    read_store_recipients, read_store_recipients_for_scope, ROOT_STORE_RECIPIENTS_SCOPE,
};
use crate::ui::ports::StoreUiPorts;
use crate::StoreRecipientsPrivateKeyRequirement;
use adw::gtk::{Button, CheckButton, Widget};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, ComboRow, NavigationPage, NavigationView, PreferencesGroup,
    ToastOverlay,
};
use keycord_keys::ui::{KeyManagementUiState, KeyRecipientWorkflowPorts};
use keycord_preferences::ui::PreferencesPageSearchState;
use keycord_runtime::i18n::gettext;
use keycord_shell::actions::register_window_action;
use keycord_shell::navigation::{WindowPageState, APP_WINDOW_TITLE};
use keycord_shell::ui::{focus_first_preferences_group_child_in_order, reveal_navigation_page};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::widgets::StoresWindowWidgets;

mod list;
mod mode;
mod save;
pub use self::save::{queue_store_recipients_autosave, register_store_recipients_save_action};
pub use crate::recipient_page::{StoreRecipientsMode, StoreRecipientsRequest};

#[derive(Clone)]
pub struct StoreRecipientsPageState {
    pub window: ApplicationWindow,
    pub nav: NavigationView,
    pub page: NavigationPage,
    pub back_row: ActionRow,
    pub search: PreferencesPageSearchState,
    pub platform: StoreRecipientsPlatformState,
    pub key_management: KeyManagementUiState,
    pub ports: StoreUiPorts,
    pub find: Button,
    pub request: Rc<RefCell<Option<StoreRecipientsRequest>>>,
    pub recipients: Rc<RefCell<Vec<String>>>,
    pub saved_recipients: Rc<RefCell<Vec<String>>>,
    pub recipient_scope_dirs: Rc<RefCell<Vec<String>>>,
    pub selected_recipient_scope: Rc<RefCell<String>>,
    pub private_key_requirement: Rc<Cell<StoreRecipientsPrivateKeyRequirement>>,
    pub saved_private_key_requirement: Rc<Cell<StoreRecipientsPrivateKeyRequirement>>,
    pub save_in_flight: Rc<Cell<bool>>,
    pub save_queued: Rc<Cell<bool>>,
    pub(crate) git_rows: Rc<RefCell<Vec<Widget>>>,
}

#[derive(Clone)]
pub struct StoreRecipientsPlatformState {
    pub overlay: ToastOverlay,
    pub scope_group: PreferencesGroup,
    pub saving_group: PreferencesGroup,
    pub scope_list: PreferencesGroup,
    pub options_group: PreferencesGroup,
    pub options_list: PreferencesGroup,
    pub scope_row: ComboRow,
    pub git_group: PreferencesGroup,
    pub git_list: PreferencesGroup,
    pub require_all_row: ActionRow,
    pub require_all_check: CheckButton,
}

impl StoreRecipientsPageState {
    /// Build Stores state from its owner bundle and reviewed subject contributions.
    pub fn new(
        widgets: &StoresWindowWidgets,
        page_state: WindowPageState,
        overlay: &ToastOverlay,
        key_management: KeyManagementUiState,
        ports: StoreUiPorts,
    ) -> Self {
        let git_rows = Rc::new(RefCell::new(Vec::new()));
        let search = widgets
            .recipient_search_state(key_management.recipient_search_groups(), git_rows.clone());

        Self {
            window: page_state.window,
            nav: page_state.nav,
            page: page_state.page,
            back_row: widgets.recipients_back_row.clone(),
            search,
            platform: widgets.recipient_platform_state(overlay),
            key_management,
            ports,
            find: page_state.find,
            request: Rc::new(RefCell::new(None)),
            recipients: Rc::new(RefCell::new(Vec::new())),
            saved_recipients: Rc::new(RefCell::new(Vec::new())),
            recipient_scope_dirs: Rc::new(RefCell::new(Vec::new())),
            selected_recipient_scope: Rc::new(RefCell::new(
                ROOT_STORE_RECIPIENTS_SCOPE.to_string(),
            )),
            private_key_requirement: Rc::new(Cell::new(
                StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
            )),
            saved_private_key_requirement: Rc::new(Cell::new(
                StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
            )),
            save_in_flight: Rc::new(Cell::new(false)),
            save_queued: Rc::new(Cell::new(false)),
            git_rows,
        }
    }

    pub fn current_request(&self) -> Option<StoreRecipientsRequest> {
        self.request.borrow().clone()
    }

    pub fn current_recipient_scope(&self) -> String {
        self.selected_recipient_scope.borrow().clone()
    }

    pub fn recipients_are_dirty(&self) -> bool {
        *self.recipients.borrow() != *self.saved_recipients.borrow()
            || self.private_key_requirement.get() != self.saved_private_key_requirement.get()
    }

    /// Clear Git-contributed rows without exposing the Stores UI row registry.
    pub fn clear_git_rows(&self) {
        keycord_shell::ui::clear_tracked_preferences_group(
            &self.platform.git_list,
            self.git_rows.as_ref(),
        );
    }

    /// Track a row contributed by the Git subject for later rebuilding.
    pub fn track_git_row(&self, row: &Widget) {
        self.git_rows.borrow_mut().push(row.clone());
    }
}

fn ordered_store_recipients_lists(state: &StoreRecipientsPageState) -> [PreferencesGroup; 7] {
    [
        state
            .key_management
            .widgets()
            .recipient_host_gpg_warning_group
            .clone(),
        state.platform.scope_list.clone(),
        state.key_management.widgets().recipient_keys_group.clone(),
        state
            .key_management
            .widgets()
            .recipient_create_group
            .clone(),
        state.key_management.widgets().recipient_add_group.clone(),
        state.platform.options_list.clone(),
        state.platform.git_list.clone(),
    ]
}

pub fn present_store_recipients_dialog(state: &StoreRecipientsPageState) {
    sync_store_recipients_page_header(state);
    reveal_navigation_page(&state.nav, &state.page);
    let _ = focus_first_preferences_group_child_in_order(&ordered_store_recipients_lists(state));
}

pub fn handle_store_recipients_subpage_back(state: &StoreRecipientsPageState) -> bool {
    state.key_management.handle_generation_subpage_back()
}

pub(super) fn load_store_recipients_scope(
    state: &StoreRecipientsPageState,
    store_root: &str,
    scope: &str,
) {
    let normalized_scope = if scope.trim().is_empty() {
        ROOT_STORE_RECIPIENTS_SCOPE
    } else {
        scope
    };
    let recipients = read_store_recipients_for_scope(store_root, normalized_scope);
    let private_key_requirement =
        read_store_private_key_requirement_for_scope(store_root, normalized_scope);
    *state.selected_recipient_scope.borrow_mut() = normalized_scope.to_string();
    *state.recipients.borrow_mut() = recipients.clone();
    *state.saved_recipients.borrow_mut() = recipients;
    state.private_key_requirement.set(private_key_requirement);
    state
        .saved_private_key_requirement
        .set(private_key_requirement);
}

pub fn connect_store_recipients_controls(state: &StoreRecipientsPageState) {
    state.back_row.set_visible(false);

    let state_for_policy = state.clone();
    let state_for_change = state.clone();
    let state_for_access_change = state.clone();
    let state_for_close = state.clone();
    state
        .key_management
        .connect_recipient_controls(KeyRecipientWorkflowPorts {
            standard_actions_allowed: Rc::new(move || {
                mode::ensure_standard_recipient_actions_allowed(&state_for_policy)
            }),
            on_key_changed: Rc::new(move || {
                rebuild_store_recipients_list(&state_for_change);
            }),
            on_key_access_changed: Rc::new(move || {
                rebuild_store_recipients_list(&state_for_access_change);
            }),
            on_generation_page_closed: Rc::new(move |reopen_recipient_page| {
                if reopen_recipient_page {
                    present_store_recipients_dialog(&state_for_close);
                } else {
                    sync_store_recipients_page_header(&state_for_close);
                }
            }),
        });
    list::connect_recipient_scope_control(state);
    list::connect_private_key_requirement_control(state);
}

pub fn rebuild_store_recipients_list(state: &StoreRecipientsPageState) {
    list::rebuild_store_recipients_list(state);
    state.search.sync();
}

pub fn register_store_recipients_reload_action(
    window: &ApplicationWindow,
    state: &StoreRecipientsPageState,
) {
    let state = state.clone();
    register_window_action(window, "reload-store-recipients-list", move || {
        if state.current_request().is_none() {
            return;
        }

        rebuild_store_recipients_list(&state);
    });
}

pub fn sync_store_recipients_page_header(state: &StoreRecipientsPageState) {
    let Some(request) = state.current_request() else {
        state.page.set_title(&gettext("Store keys"));
        (state.ports.navigation.show_secondary_page)("Store keys", APP_WINDOW_TITLE, false);
        state.find.set_visible(true);
        return;
    };

    state.page.set_title(&gettext(request.mode.page_title()));
    (state.ports.navigation.show_secondary_page)(request.mode.page_title(), &request.store, false);
    state.find.set_visible(true);
}

fn show_store_recipients_page(
    state: &StoreRecipientsPageState,
    request: StoreRecipientsRequest,
    initial_recipients: Vec<String>,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) {
    let mode = request.mode;
    *state.request.borrow_mut() = Some(request);
    *state.recipient_scope_dirs.borrow_mut() = Vec::new();
    *state.selected_recipient_scope.borrow_mut() = ROOT_STORE_RECIPIENTS_SCOPE.to_string();
    *state.recipients.borrow_mut() = initial_recipients.clone();
    *state.saved_recipients.borrow_mut() = initial_recipients;
    state.private_key_requirement.set(private_key_requirement);
    state
        .saved_private_key_requirement
        .set(private_key_requirement);
    state.save_in_flight.set(false);
    state.save_queued.set(false);
    state.key_management.reset_recipient_navigation();
    state.platform.options_group.set_visible(true);
    rebuild_store_recipients_list(state);
    state.search.sync();
    present_store_recipients_dialog(state);

    if mode.creates_store() {
        queue_store_recipients_autosave(state);
    }
}

pub fn show_store_recipients_create_page(
    state: &StoreRecipientsPageState,
    store: impl Into<String>,
    initial_recipients: Vec<String>,
) {
    show_store_recipients_page(
        state,
        StoreRecipientsRequest::create(store),
        initial_recipients,
        StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
    );
}

pub fn show_store_recipients_edit_page(state: &StoreRecipientsPageState, store: impl Into<String>) {
    let store = store.into();
    show_store_recipients_page(
        state,
        StoreRecipientsRequest::edit(store.clone()),
        read_store_recipients(&store),
        read_store_private_key_requirement(&store),
    );
}

#[cfg(test)]
mod tests {
    use super::StoreRecipientsMode;

    #[test]
    fn mode_titles_match_their_behavior() {
        assert_eq!(StoreRecipientsMode::Create.page_title(), "New Store");
        assert_eq!(StoreRecipientsMode::Edit.page_title(), "Store keys");
    }

    #[test]
    fn mode_messages_match_their_behavior() {
        assert_eq!(
            StoreRecipientsMode::Create.empty_state_subtitle(),
            "Add at least one recipient to create this store."
        );
        assert_eq!(
            StoreRecipientsMode::Edit.save_failure_message(),
            "Couldn't save store keys."
        );
    }
}
