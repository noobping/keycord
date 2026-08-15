use crate::recipients::{
    read_store_private_key_requirement_for_scope, read_store_recipients_for_scope,
    relevant_store_recipient_scopes, ROOT_STORE_RECIPIENTS_SCOPE,
};
use crate::ui::ports::StoreUiPorts;
use crate::StoreRecipientsPrivateKeyRequirement;
use adw::gtk::{Box as GtkBox, Button, CheckButton, Stack, Widget};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, ComboRow, NavigationPage, NavigationView, PreferencesGroup,
    StatusPage, ToastOverlay,
};
use keycord_keys::ui::{KeyManagementUiState, KeyRecipientWorkflowPorts};
use keycord_preferences::ui::PreferencesPageSearchState;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::actions::register_window_action;
use keycord_shell::background::spawn_result_task;
use keycord_shell::navigation::{WindowPageState, APP_WINDOW_TITLE};
use keycord_shell::ui::{
    focus_first_preferences_group_child_in_order, reveal_navigation_page,
    visible_navigation_page_is,
};
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
    load_generation: Rc<Cell<u64>>,
    loading: Rc<Cell<bool>>,
    pub(crate) git_rows: Rc<RefCell<Vec<Widget>>>,
}

#[derive(Clone)]
pub struct StoreRecipientsPlatformState {
    pub overlay: ToastOverlay,
    pub recipients_stack: Stack,
    pub recipients_content: GtkBox,
    pub recipients_loading: StatusPage,
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

impl StoreRecipientsPlatformState {
    pub fn show_loading(&self) {
        self.recipients_stack
            .set_visible_child(&self.recipients_loading);
    }

    pub fn show_content(&self) {
        self.recipients_stack
            .set_visible_child(&self.recipients_content);
    }
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
            load_generation: Rc::new(Cell::new(0)),
            loading: Rc::new(Cell::new(false)),
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

    pub fn is_loading(&self) -> bool {
        self.loading.get()
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

#[derive(Clone)]
enum StoreRecipientValuesLoad {
    KeepCurrent,
    ReadFromStore,
    Provided {
        recipients: Vec<String>,
        private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    },
}

struct StoreRecipientsPageLoad {
    scopes: Vec<String>,
    selected_scope: String,
    values: Option<(Vec<String>, StoreRecipientsPrivateKeyRequirement)>,
}

fn next_load_generation(current: u64) -> u64 {
    current.wrapping_add(1).max(1)
}

fn normalized_loaded_scopes(mut scopes: Vec<String>) -> Vec<String> {
    if scopes.is_empty() {
        scopes.push(ROOT_STORE_RECIPIENTS_SCOPE.to_string());
    }
    scopes
}

fn load_store_recipients_page(
    request: &StoreRecipientsRequest,
    uses_integrated_backend: bool,
    current_scope: String,
    values: StoreRecipientValuesLoad,
) -> StoreRecipientsPageLoad {
    let mut scopes = normalized_loaded_scopes(if uses_integrated_backend {
        relevant_store_recipient_scopes(&request.store)
    } else {
        Vec::new()
    });
    if matches!(&values, StoreRecipientValuesLoad::Provided { .. })
        && !scopes.iter().any(|scope| scope == &current_scope)
    {
        scopes.insert(0, current_scope.clone());
    }
    let selected_scope = if scopes.iter().any(|scope| scope == &current_scope) {
        current_scope.clone()
    } else {
        scopes
            .first()
            .cloned()
            .unwrap_or_else(|| ROOT_STORE_RECIPIENTS_SCOPE.to_string())
    };
    let values = match values {
        StoreRecipientValuesLoad::KeepCurrent if selected_scope == current_scope => None,
        StoreRecipientValuesLoad::KeepCurrent | StoreRecipientValuesLoad::ReadFromStore => Some((
            read_store_recipients_for_scope(&request.store, &selected_scope),
            read_store_private_key_requirement_for_scope(&request.store, &selected_scope),
        )),
        StoreRecipientValuesLoad::Provided {
            recipients,
            private_key_requirement,
        } => Some((recipients, private_key_requirement)),
    };
    StoreRecipientsPageLoad {
        scopes,
        selected_scope,
        values,
    }
}

fn begin_store_recipients_loading(state: &StoreRecipientsPageState) -> u64 {
    let generation = next_load_generation(state.load_generation.get());
    state.load_generation.set(generation);
    state.loading.set(true);
    state.platform.show_loading();
    if visible_navigation_page_is(&state.nav, &state.page) {
        state.find.set_visible(false);
    }
    generation
}

fn store_recipients_load_is_current(
    state: &StoreRecipientsPageState,
    generation: u64,
    request: &StoreRecipientsRequest,
) -> bool {
    state.load_generation.get() == generation && state.current_request().as_ref() == Some(request)
}

fn finish_store_recipients_loading(
    state: &StoreRecipientsPageState,
    generation: u64,
    request: &StoreRecipientsRequest,
) {
    if !store_recipients_load_is_current(state, generation, request) {
        return;
    }
    state.loading.set(false);
    state.platform.show_content();
    state.search.sync();
    if visible_navigation_page_is(&state.nav, &state.page) {
        sync_store_recipients_page_header(state);
        let _ =
            focus_first_preferences_group_child_in_order(&ordered_store_recipients_lists(state));
    }
}

pub fn present_store_recipients_dialog(state: &StoreRecipientsPageState) {
    sync_store_recipients_page_header(state);
    reveal_navigation_page(&state.nav, &state.page);
    if !state.is_loading() {
        let _ =
            focus_first_preferences_group_child_in_order(&ordered_store_recipients_lists(state));
    }
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
                refresh_store_recipients_key_inventory(&state_for_change);
            }),
            on_key_access_changed: Rc::new(move || {
                refresh_store_recipients_key_inventory(&state_for_access_change);
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

fn refresh_store_recipients_key_inventory(state: &StoreRecipientsPageState) {
    let Some(request) = state.current_request() else {
        return;
    };
    let generation = begin_store_recipients_loading(state);
    let state_for_loaded = state.clone();
    let request_for_loaded = request.clone();
    list::refresh_store_recipients_list(
        state,
        Rc::new(move || {
            finish_store_recipients_loading(&state_for_loaded, generation, &request_for_loaded);
        }),
    );
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

        start_store_recipients_page_load(&state, StoreRecipientValuesLoad::KeepCurrent, false);
    });
}

pub fn sync_store_recipients_page_header(state: &StoreRecipientsPageState) {
    let Some(request) = state.current_request() else {
        state.page.set_title(&gettext("Store keys"));
        (state.ports.navigation.show_secondary_page)("Store keys", APP_WINDOW_TITLE, false);
        state.find.set_visible(!state.is_loading());
        return;
    };

    state.page.set_title(&gettext(request.mode.page_title()));
    (state.ports.navigation.show_secondary_page)(request.mode.page_title(), &request.store, false);
    state.find.set_visible(!state.is_loading());
}

fn apply_store_recipients_page_load(
    state: &StoreRecipientsPageState,
    load: StoreRecipientsPageLoad,
) {
    *state.recipient_scope_dirs.borrow_mut() = load.scopes;
    *state.selected_recipient_scope.borrow_mut() = load.selected_scope;
    if let Some((recipients, private_key_requirement)) = load.values {
        *state.recipients.borrow_mut() = recipients.clone();
        *state.saved_recipients.borrow_mut() = recipients;
        state.private_key_requirement.set(private_key_requirement);
        state
            .saved_private_key_requirement
            .set(private_key_requirement);
    }
}

fn fallback_store_recipients_page_load(
    values: StoreRecipientValuesLoad,
) -> StoreRecipientsPageLoad {
    let values = match values {
        StoreRecipientValuesLoad::Provided {
            recipients,
            private_key_requirement,
        } => Some((recipients, private_key_requirement)),
        StoreRecipientValuesLoad::KeepCurrent | StoreRecipientValuesLoad::ReadFromStore => None,
    };
    StoreRecipientsPageLoad {
        scopes: vec![ROOT_STORE_RECIPIENTS_SCOPE.to_string()],
        selected_scope: ROOT_STORE_RECIPIENTS_SCOPE.to_string(),
        values,
    }
}

fn complete_store_recipients_page_load(
    state: &StoreRecipientsPageState,
    generation: u64,
    request: StoreRecipientsRequest,
    load: StoreRecipientsPageLoad,
    queue_initial_autosave: bool,
) {
    if !store_recipients_load_is_current(state, generation, &request) {
        return;
    }
    apply_store_recipients_page_load(state, load);
    let state_for_loaded = state.clone();
    let request_for_loaded = request.clone();
    list::refresh_store_recipients_list(
        state,
        Rc::new(move || {
            finish_store_recipients_loading(&state_for_loaded, generation, &request_for_loaded);
        }),
    );
    if queue_initial_autosave && request.mode.creates_store() {
        queue_store_recipients_autosave(state);
    }
}

fn start_store_recipients_page_load(
    state: &StoreRecipientsPageState,
    values: StoreRecipientValuesLoad,
    queue_initial_autosave: bool,
) {
    let Some(request) = state.current_request() else {
        return;
    };
    let generation = begin_store_recipients_loading(state);
    let uses_integrated_backend = state.ports.preferences.uses_integrated_backend();
    let current_scope = state.current_recipient_scope();
    let request_for_worker = request.clone();
    let values_for_worker = values.clone();
    let state_for_result = state.clone();
    let request_for_result = request.clone();
    let state_for_disconnect = state.clone();
    let request_for_disconnect = request;
    spawn_result_task(
        move || {
            load_store_recipients_page(
                &request_for_worker,
                uses_integrated_backend,
                current_scope,
                values_for_worker,
            )
        },
        move |load| {
            complete_store_recipients_page_load(
                &state_for_result,
                generation,
                request_for_result,
                load,
                queue_initial_autosave,
            );
        },
        move || {
            log_error("Store-key page loader disconnected unexpectedly.");
            complete_store_recipients_page_load(
                &state_for_disconnect,
                generation,
                request_for_disconnect,
                fallback_store_recipients_page_load(values),
                queue_initial_autosave,
            );
        },
    );
}

fn show_store_recipients_page(
    state: &StoreRecipientsPageState,
    request: StoreRecipientsRequest,
    values: StoreRecipientValuesLoad,
) {
    *state.request.borrow_mut() = Some(request.clone());
    *state.recipient_scope_dirs.borrow_mut() = Vec::new();
    *state.selected_recipient_scope.borrow_mut() = ROOT_STORE_RECIPIENTS_SCOPE.to_string();
    if matches!(&values, StoreRecipientValuesLoad::ReadFromStore) {
        state.recipients.borrow_mut().clear();
        state.saved_recipients.borrow_mut().clear();
        state
            .private_key_requirement
            .set(StoreRecipientsPrivateKeyRequirement::AnyManagedKey);
        state
            .saved_private_key_requirement
            .set(StoreRecipientsPrivateKeyRequirement::AnyManagedKey);
    }
    state.save_in_flight.set(false);
    state.save_queued.set(false);
    state.key_management.reset_recipient_navigation();
    state.platform.options_group.set_visible(true);
    start_store_recipients_page_load(state, values, request.mode.creates_store());
    present_store_recipients_dialog(state);
}

pub fn show_store_recipients_create_page(
    state: &StoreRecipientsPageState,
    store: impl Into<String>,
    initial_recipients: Vec<String>,
) {
    show_store_recipients_page(
        state,
        StoreRecipientsRequest::create(store),
        StoreRecipientValuesLoad::Provided {
            recipients: initial_recipients,
            private_key_requirement: StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        },
    );
}

pub fn show_store_recipients_edit_page(state: &StoreRecipientsPageState, store: impl Into<String>) {
    show_store_recipients_page(
        state,
        StoreRecipientsRequest::edit(store),
        StoreRecipientValuesLoad::ReadFromStore,
    );
}

#[cfg(test)]
mod tests {
    use super::{next_load_generation, normalized_loaded_scopes, StoreRecipientsMode};

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

    #[test]
    fn page_load_generations_never_use_the_idle_generation() {
        assert_eq!(next_load_generation(0), 1);
        assert_eq!(next_load_generation(41), 42);
        assert_eq!(next_load_generation(u64::MAX), 1);
    }

    #[test]
    fn a_store_without_discovered_scopes_still_loads_the_default_scope() {
        assert_eq!(normalized_loaded_scopes(Vec::new()), ["."]);
        assert_eq!(normalized_loaded_scopes(vec!["team".to_string()]), ["team"]);
    }
}
