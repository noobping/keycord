use super::remote::{present_remote_dialog, RemoteDialogRequest};
use crate::{
    add_store_git_remote, list_store_git_remotes, remove_store_git_remote, rename_store_git_remote,
    set_store_git_remote_url, store_git_repository_status, sync_store_repository, StoreGitHead,
    StoreGitRepositoryStatus,
};
use adw::gtk::{Button, Image, Widget};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, NavigationPage, NavigationView, PreferencesGroup, StatusPage,
    Toast, ToastOverlay, WindowTitle,
};
use keycord_preferences::ui::PreferencesPageSearchState;
use keycord_runtime::capabilities::{has_host_permission, supports_host_command_features};
use keycord_runtime::{i18n::gettext, log_error};
use keycord_shell::background::{spawn_result_task, spawn_result_task_with_finalizer};
use keycord_shell::navigation::{
    finish_transient_navigation_page, push_navigation_page_if_needed, reveal_navigation_page,
    show_secondary_page_chrome, HasWindowChrome, WindowChrome, WindowPageState, APP_WINDOW_TITLE,
};
use keycord_shell::object_data::{cloned_data, set_cloned_data};
use keycord_shell::ui::{
    add_tracked_preferences_group_child, append_action_group_row_with_button,
    append_info_group_row, clear_tracked_preferences_group, dim_label_icon,
    flat_icon_button_with_tooltip, focus_first_preferences_group_child_in_order,
};
use keycord_stores::ui::recipient_page::{StoreRecipientsMode, StoreRecipientsPageState};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::window_widgets::GitWindowWidgets;

const STORE_RECIPIENTS_GIT_ROW_REFRESH_ID_KEY: &str = "keycord-store-recipients-git-row-refresh-id";

pub type AppendOptionalHostAccessRow =
    Rc<dyn Fn(&PreferencesGroup, &ToastOverlay) -> Option<ActionRow>>;

#[derive(Clone)]
pub struct StoreGitPagePorts {
    pub append_optional_host_access_row: AppendOptionalHostAccessRow,
    pub set_application_busy: Rc<dyn Fn(bool)>,
    pub refresh_related_views: Rc<dyn Fn()>,
}

#[derive(Clone)]
pub struct StoreGitPageState {
    pub window: ApplicationWindow,
    pub nav: NavigationView,
    pub page: NavigationPage,
    pub back_row: ActionRow,
    pub search: PreferencesPageSearchState,
    pub remotes_list: PreferencesGroup,
    pub actions_list: PreferencesGroup,
    pub status_list: PreferencesGroup,
    pub access_list: PreferencesGroup,
    pub overlay: ToastOverlay,
    pub back: Button,
    pub add: Button,
    pub find: Button,
    pub primary_action: Button,
    pub secondary_action: Button,
    pub save: Button,
    pub raw: Button,
    pub title: WindowTitle,
    pub busy_page: NavigationPage,
    pub busy_status: StatusPage,
    pub current_store: Rc<RefCell<Option<String>>>,
    pub recipients_page: Rc<RefCell<Option<StoreRecipientsPageState>>>,
    pub reopen_after_busy: Rc<Cell<bool>>,
    pub remote_rows: Rc<RefCell<Vec<Widget>>>,
    pub action_rows: Rc<RefCell<Vec<Widget>>>,
    pub status_rows: Rc<RefCell<Vec<Widget>>>,
    pub ports: StoreGitPagePorts,
}

impl StoreGitPageState {
    /// Build Git state from its owner bundle and application-supplied chrome/ports.
    pub fn new(
        widgets: &GitWindowWidgets,
        page_state: WindowPageState,
        overlay: &ToastOverlay,
        ports: StoreGitPagePorts,
    ) -> Self {
        let remote_rows = Rc::new(RefCell::new(Vec::new()));
        let action_rows = Rc::new(RefCell::new(Vec::new()));
        let status_rows = Rc::new(RefCell::new(Vec::new()));
        let search = widgets.store_page_search_state(
            remote_rows.clone(),
            action_rows.clone(),
            status_rows.clone(),
        );

        Self {
            window: page_state.window,
            nav: page_state.nav,
            page: page_state.page,
            back_row: widgets.store_git_back_row.clone(),
            search,
            remotes_list: widgets.store_git_remotes_list.clone(),
            actions_list: widgets.store_git_actions_list.clone(),
            status_list: widgets.store_git_status_list.clone(),
            access_list: widgets.store_git_access_list.clone(),
            overlay: overlay.clone(),
            back: page_state.back,
            add: page_state.add,
            find: page_state.find,
            primary_action: page_state.primary_action,
            secondary_action: page_state.secondary_action,
            save: page_state.save,
            raw: page_state.raw,
            title: page_state.title,
            busy_page: widgets.git_busy_page.clone(),
            busy_status: widgets.git_busy_status.clone(),
            current_store: Rc::new(RefCell::new(None)),
            recipients_page: Rc::new(RefCell::new(None)),
            reopen_after_busy: Rc::new(Cell::new(false)),
            remote_rows,
            action_rows,
            status_rows,
            ports,
        }
    }

    pub fn current_store(&self) -> Option<String> {
        self.current_store.borrow().clone()
    }
}

impl HasWindowChrome for StoreGitPageState {
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

fn begin_git_operation(state: &StoreGitPageState, title: &str) {
    super::actions::set_git_operation_busy(
        &state.window,
        state.ports.set_application_busy.as_ref(),
        true,
    );
    state.reopen_after_busy.set(true);
    state.busy_status.set_title(&gettext(title));
    push_navigation_page_if_needed(&state.nav, &state.busy_page);
}

fn finish_git_operation(state: &StoreGitPageState) {
    super::actions::set_git_operation_busy(
        &state.window,
        state.ports.set_application_busy.as_ref(),
        false,
    );
    finish_transient_navigation_page(&state.nav, &state.busy_page);

    if state.reopen_after_busy.replace(false) {
        sync_store_git_page_header(state);
        state.search.sync();
    }
}

fn ordered_store_git_lists(state: &StoreGitPageState) -> [PreferencesGroup; 4] {
    [
        state.remotes_list.clone(),
        state.actions_list.clone(),
        state.status_list.clone(),
        state.access_list.clone(),
    ]
}

pub fn present_store_git_dialog(state: &StoreGitPageState) {
    sync_store_git_page_header(state);
    reveal_navigation_page(&state.nav, &state.page);
    let _ = focus_first_preferences_group_child_in_order(&ordered_store_git_lists(state));
}

pub fn connect_store_git_controls(state: &StoreGitPageState) {
    state.back_row.set_visible(false);
}

fn append_status_row(
    list: &PreferencesGroup,
    title: &str,
    subtitle: &str,
    icon_name: &str,
) -> ActionRow {
    let title = gettext(title);
    let row = ActionRow::builder()
        .title(&title)
        .subtitle(subtitle)
        .build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon(icon_name));
    list.add(&row);
    row
}

fn translated_branch_message(template: &str, branch: &str) -> String {
    gettext(template).replace("{branch}", branch)
}

fn translated_count_message(template: &str, count: usize) -> String {
    gettext(template).replace("{count}", &count.to_string())
}

fn append_translated_action_row_with_button(
    list: &PreferencesGroup,
    title: &str,
    subtitle: &str,
    icon_name: &str,
    action: impl Fn() + 'static,
) -> ActionRow {
    let row = ActionRow::builder().title(title).subtitle(subtitle).build();
    row.set_activatable(true);

    let icon = Image::from_icon_name(icon_name);
    row.add_suffix(&icon);
    list.add(&row);

    let action = Rc::new(action);
    let row_action = action.clone();
    row.connect_activated(move |_| row_action());

    row
}

fn repository_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if !status.has_repository {
        return gettext("No Git repository yet. Add a remote to initialize one.");
    }
    if status.dirty && status.has_outgoing_commits && status.has_incoming_commits {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync, and local and remote commits are waiting to sync.",
        );
    }
    if status.dirty && status.has_outgoing_commits {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync, and local commits are waiting to sync.",
        );
    }
    if status.dirty && status.has_incoming_commits {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync, and remote commits are waiting to sync.",
        );
    }
    if status.dirty {
        return gettext(
            "Repository found. Local changes must be committed or discarded before sync.",
        );
    }

    match &status.head {
        StoreGitHead::Branch(_) if status.has_outgoing_commits && status.has_incoming_commits => {
            gettext("Repository found. Local and remote commits are waiting to sync.")
        }
        StoreGitHead::Branch(_) if status.has_outgoing_commits => {
            gettext("Repository found. Local commits are waiting to sync.")
        }
        StoreGitHead::Branch(_) if status.has_incoming_commits => {
            gettext("Repository found. Remote commits are waiting to sync.")
        }
        StoreGitHead::Branch(_) => gettext("Repository found and ready for remote management."),
        StoreGitHead::UnbornBranch(branch) => translated_branch_message(
            "Repository found. Create the first commit on '{branch}' before syncing.",
            branch,
        ),
        StoreGitHead::Detached => gettext("Repository found. Check out a branch before syncing."),
    }
}

fn branch_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if !status.has_repository {
        return gettext("No branch yet.");
    }

    match &status.head {
        StoreGitHead::Branch(branch) => branch.clone(),
        StoreGitHead::UnbornBranch(branch) => {
            translated_branch_message("{branch} (no commits yet)", branch)
        }
        StoreGitHead::Detached => gettext("Detached HEAD"),
    }
}

fn remote_count_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if status.has_outgoing_commits && status.has_incoming_commits {
        return gettext("Local and remote commits are waiting to sync.");
    }
    if status.has_outgoing_commits {
        return gettext("Local commits are waiting to sync.");
    }
    if status.has_incoming_commits {
        return gettext("Remote commits are waiting to sync.");
    }

    match status.remotes.len() {
        0 => gettext("No remotes configured."),
        1 => gettext("1 remote configured."),
        count => translated_count_message("{count} remotes configured.", count),
    }
}

fn sync_allowed(status: &StoreGitRepositoryStatus) -> bool {
    has_host_permission()
        && status.has_repository
        && !status.remotes.is_empty()
        && !status.dirty
        && matches!(status.head, StoreGitHead::Branch(_))
}

fn sync_subtitle(status: &StoreGitRepositoryStatus) -> String {
    if !has_host_permission() {
        return gettext("Grant host access to fetch, merge, and push.");
    }
    if !status.has_repository {
        return gettext("Add a remote to initialize a Git repository first.");
    }
    if status.remotes.is_empty() {
        return gettext("Add at least one remote before syncing.");
    }
    if status.dirty && status.has_outgoing_commits && status.has_incoming_commits {
        return gettext(
            "Commit or discard local changes before syncing. Local and remote commits are also waiting to sync.",
        );
    }
    if status.dirty && status.has_outgoing_commits {
        return gettext(
            "Commit or discard local changes before syncing. Local commits are also waiting to sync.",
        );
    }
    if status.dirty && status.has_incoming_commits {
        return gettext(
            "Commit or discard local changes before syncing. Remote commits are also waiting to sync.",
        );
    }
    if status.dirty {
        return gettext("Commit or discard local changes before syncing.");
    }

    match &status.head {
        StoreGitHead::Branch(branch)
            if status.has_outgoing_commits && status.has_incoming_commits =>
        {
            translated_branch_message(
                "Local and remote commits are waiting to sync on '{branch}'.",
                branch,
            )
        }
        StoreGitHead::Branch(branch) if status.has_outgoing_commits => {
            translated_branch_message("Local commits are ready to push on '{branch}'.", branch)
        }
        StoreGitHead::Branch(branch) if status.has_incoming_commits => {
            translated_branch_message("Remote commits are ready to merge into '{branch}'.", branch)
        }
        StoreGitHead::Branch(branch) => translated_branch_message(
            "Fetch, merge, and push the current '{branch}' branch across all remotes.",
            branch,
        ),
        StoreGitHead::UnbornBranch(branch) => translated_branch_message(
            "Make an initial commit on '{branch}' before syncing.",
            branch,
        ),
        StoreGitHead::Detached => gettext("Check out a branch before syncing."),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoreGitRowState {
    subtitle: String,
    enabled: bool,
}

fn store_git_row_state(status: Result<StoreGitRepositoryStatus, String>) -> StoreGitRowState {
    match status {
        Ok(status) if sync_allowed(&status) => StoreGitRowState {
            subtitle: remote_count_subtitle(&status),
            enabled: true,
        },
        Ok(status) => StoreGitRowState {
            subtitle: sync_subtitle(&status),
            enabled: true,
        },
        Err(_) => StoreGitRowState {
            subtitle: gettext("Couldn't inspect Git remotes."),
            enabled: false,
        },
    }
}

fn store_git_row_state_for_store(store: &str) -> StoreGitRowState {
    store_git_row_state(store_git_repository_status(store))
}

fn next_store_recipients_git_row_refresh_id(list: &PreferencesGroup) -> u64 {
    let refresh_id = cloned_data::<_, u64>(list, STORE_RECIPIENTS_GIT_ROW_REFRESH_ID_KEY)
        .unwrap_or_default()
        .wrapping_add(1)
        .max(1);
    set_cloned_data(list, STORE_RECIPIENTS_GIT_ROW_REFRESH_ID_KEY, refresh_id);
    refresh_id
}

fn store_recipients_git_row_refresh_is_current(
    state: &StoreRecipientsPageState,
    refresh_id: u64,
    store: &str,
) -> bool {
    cloned_data::<_, u64>(
        &state.platform.git_list,
        STORE_RECIPIENTS_GIT_ROW_REFRESH_ID_KEY,
    ) == Some(refresh_id)
        && state.current_request().is_some_and(|request| {
            request.store == store
                && matches!(
                    request.mode,
                    StoreRecipientsMode::Edit | StoreRecipientsMode::Create
                )
        })
}

fn apply_store_recipients_git_row_state(row: &ActionRow, row_state: &StoreGitRowState) {
    row.set_subtitle(&row_state.subtitle);
    row.set_sensitive(row_state.enabled);
    row.set_activatable(row_state.enabled);
}

fn update_store_git_remote(
    store: &str,
    current_name: &str,
    next_name: &str,
    next_url: &str,
) -> Result<(), String> {
    let name_changed = current_name != next_name;
    let current_url = list_store_git_remotes(store)?
        .into_iter()
        .find(|remote| remote.name == current_name)
        .map(|remote| remote.url)
        .unwrap_or_default();
    let url_changed = current_url != next_url;

    if !name_changed && !url_changed {
        return Ok(());
    }
    if name_changed {
        rename_store_git_remote(store, current_name, next_name)?;
    }
    if url_changed {
        if let Err(err) = set_store_git_remote_url(store, next_name, next_url) {
            if name_changed {
                let _ = rename_store_git_remote(store, next_name, current_name);
            }
            return Err(err);
        }
    }

    Ok(())
}

fn sync_related_views(state: &StoreGitPageState) {
    (state.ports.refresh_related_views)();
}

fn append_remote_row(
    state: &StoreGitPageState,
    store: &str,
    name: &str,
    url: &str,
    existing_names: Vec<String>,
    existing_urls: Vec<String>,
) {
    let row = ActionRow::builder().title(name).subtitle(url).build();
    row.set_activatable(false);
    row.add_prefix(&dim_label_icon("git-symbolic"));

    let edit_button = flat_icon_button_with_tooltip("edit-symbolic", "Edit remote");
    row.add_suffix(&edit_button);

    let delete_button = flat_icon_button_with_tooltip("user-trash-symbolic", "Remove remote");
    row.add_suffix(&delete_button);

    add_tracked_preferences_group_child(&state.remotes_list, state.remote_rows.as_ref(), &row);

    let store_for_edit = store.to_string();
    let state_for_edit = state.clone();
    let current_name = name.to_string();
    let current_url = url.to_string();
    edit_button.connect_clicked(move |_| {
        let state_for_submit = state_for_edit.clone();
        let store_for_submit = store_for_edit.clone();
        let current_name_for_submit = current_name.clone();
        present_remote_dialog(
            RemoteDialogRequest {
                window: &state_for_edit.window,
                store: &store_for_edit,
                title: "Edit remote",
                initial_name: &current_name,
                initial_url: &current_url,
                existing_names: existing_names.clone(),
                existing_urls: existing_urls.clone(),
            },
            move |next_name, next_url| {
                update_store_git_remote(
                    &store_for_submit,
                    &current_name_for_submit,
                    &next_name,
                    &next_url,
                )?;
                rebuild_store_git_page(&state_for_submit);
                sync_related_views(&state_for_submit);
                state_for_submit
                    .overlay
                    .add_toast(Toast::new(&gettext("Remote updated.")));
                Ok(())
            },
        );
    });

    let store_for_delete = store.to_string();
    let state_for_delete = state.clone();
    let name_for_delete = name.to_string();
    delete_button.connect_clicked(move |_| {
        match remove_store_git_remote(&store_for_delete, &name_for_delete) {
            Ok(()) => {
                rebuild_store_git_page(&state_for_delete);
                sync_related_views(&state_for_delete);
                state_for_delete
                    .overlay
                    .add_toast(Toast::new(&gettext("Remote removed.")));
            }
            Err(err) => {
                log_error(format!(
                    "Failed to remove Git remote '{name_for_delete}' from '{store_for_delete}': {err}"
                ));
                state_for_delete
                    .overlay
                    .add_toast(Toast::new(&gettext("Couldn't remove that remote.")));
            }
        }
    });
}

pub fn rebuild_store_git_page(state: &StoreGitPageState) {
    clear_tracked_preferences_group(&state.remotes_list, state.remote_rows.as_ref());
    clear_tracked_preferences_group(&state.actions_list, state.action_rows.as_ref());
    clear_tracked_preferences_group(&state.status_list, state.status_rows.as_ref());
    state.access_list.set_visible(false);

    let Some(store) = state.current_store() else {
        let row = append_info_group_row(
            &state.remotes_list,
            "No password store",
            "Open a store first.",
        );
        state.remote_rows.borrow_mut().push(row.upcast());
        return;
    };

    match store_git_repository_status(&store) {
        Ok(status) => {
            let existing_remote_names = status
                .remotes
                .iter()
                .map(|remote| remote.name.clone())
                .collect::<Vec<_>>();
            let existing_remote_urls = status
                .remotes
                .iter()
                .map(|remote| remote.url.clone())
                .collect::<Vec<_>>();
            if status.remotes.is_empty() {
                let row = append_status_row(
                    &state.remotes_list,
                    "Repository",
                    &repository_subtitle(&status),
                    "git-symbolic",
                );
                state.remote_rows.borrow_mut().push(row.upcast());
            } else {
                for remote in &status.remotes {
                    append_remote_row(
                        state,
                        &store,
                        &remote.name,
                        &remote.url,
                        existing_remote_names
                            .iter()
                            .filter(|existing_name| {
                                !existing_name.eq_ignore_ascii_case(&remote.name)
                            })
                            .cloned()
                            .collect(),
                        status
                            .remotes
                            .iter()
                            .filter(|existing_remote| existing_remote.name != remote.name)
                            .map(|existing_remote| existing_remote.url.clone())
                            .collect(),
                    );
                }
            }

            let add_state = state.clone();
            let store_for_add = store.clone();
            let add_row = append_action_group_row_with_button(
                &state.actions_list,
                "Add remote",
                "Add a Git remote for this store.",
                "list-add-symbolic",
                move || {
                    let state_for_submit = add_state.clone();
                    let store_for_submit = store_for_add.clone();
                    present_remote_dialog(
                        RemoteDialogRequest {
                            window: &add_state.window,
                            store: &store_for_add,
                            title: "Add remote",
                            initial_name: "",
                            initial_url: "",
                            existing_names: existing_remote_names.clone(),
                            existing_urls: existing_remote_urls.clone(),
                        },
                        move |name, url| {
                            add_store_git_remote(&store_for_submit, &name, &url)?;
                            rebuild_store_git_page(&state_for_submit);
                            sync_related_views(&state_for_submit);
                            state_for_submit
                                .overlay
                                .add_toast(Toast::new(&gettext("Remote added.")));
                            Ok(())
                        },
                    );
                },
            );
            state
                .action_rows
                .borrow_mut()
                .push(add_row.clone().upcast());
            add_row.set_sensitive(has_host_permission());
            add_row.set_activatable(has_host_permission());

            let _ =
                (state.ports.append_optional_host_access_row)(&state.access_list, &state.overlay);

            let sync_state = state.clone();
            let store_for_sync = store.clone();
            let sync_row = append_translated_action_row_with_button(
                &state.status_list,
                &gettext("Sync now"),
                &sync_subtitle(&status),
                "view-refresh-symbolic",
                move || {
                    let current_status = match store_git_repository_status(&store_for_sync) {
                        Ok(status) => status,
                        Err(err) => {
                            log_error(format!(
                                "Failed to inspect Git state before syncing '{store_for_sync}': {err}"
                            ));
                            sync_state
                                .overlay
                                .add_toast(Toast::new(&gettext("Couldn't inspect Git remotes.")));
                            rebuild_store_git_page(&sync_state);
                            return;
                        }
                    };
                    if !sync_allowed(&current_status) {
                        sync_state
                            .overlay
                            .add_toast(Toast::new(&sync_subtitle(&current_status)));
                        rebuild_store_git_page(&sync_state);
                        return;
                    }

                    begin_git_operation(&sync_state, "Syncing store");

                    let state_for_finalize = sync_state.clone();
                    let state_for_result = sync_state.clone();
                    let state_for_disconnect = sync_state.clone();
                    let store_for_worker = store_for_sync.clone();
                    let store_for_result = store_for_sync.clone();
                    spawn_result_task_with_finalizer(
                        move || sync_store_repository(&store_for_worker),
                        move || {
                            finish_git_operation(&state_for_finalize);
                            rebuild_store_git_page(&state_for_finalize);
                            sync_related_views(&state_for_finalize);
                        },
                        move |result| match result {
                            Ok(()) => {
                                state_for_result
                                    .overlay
                                    .add_toast(Toast::new(&gettext("Store synced.")));
                            }
                            Err(err) => {
                                log_error(format!(
                                    "Failed to sync password store '{store_for_result}': {err}"
                                ));
                                state_for_result
                                    .overlay
                                    .add_toast(Toast::new(&gettext("Couldn't sync store.")));
                            }
                        },
                        move || {
                            state_for_disconnect.overlay.add_toast(Toast::new(&gettext(
                                "Store sync stopped unexpectedly.",
                            )));
                        },
                    );
                },
            );
            state
                .status_rows
                .borrow_mut()
                .push(sync_row.clone().upcast());
            sync_row.set_sensitive(sync_allowed(&status));
            sync_row.set_activatable(sync_allowed(&status));

            let row = append_status_row(
                &state.status_list,
                "Branch",
                &branch_subtitle(&status),
                "object-select-symbolic",
            );
            state.status_rows.borrow_mut().push(row.upcast());
        }
        Err(err) => {
            log_error(format!("Failed to inspect Git state for '{store}': {err}"));
            let row = append_info_group_row(
                &state.remotes_list,
                "Couldn't inspect Git state",
                "Check the logs for details.",
            );
            state.remote_rows.borrow_mut().push(row.upcast());
        }
    }

    state.search.sync();
}

pub fn sync_store_git_page_header(state: &StoreGitPageState) {
    let chrome = state.window_chrome();
    let Some(store) = state.current_store() else {
        state.page.set_title(&gettext("Git remotes"));
        show_secondary_page_chrome(&chrome, "Git remotes", APP_WINDOW_TITLE, false);
        chrome.find.set_visible(true);
        return;
    };

    state.page.set_title(&gettext("Git remotes"));
    show_secondary_page_chrome(&chrome, "Git remotes", &store, false);
    chrome.find.set_visible(true);
}

fn show_store_git_page_with_back_destination(state: &StoreGitPageState, store: impl Into<String>) {
    if !supports_host_command_features() {
        return;
    }

    *state.current_store.borrow_mut() = Some(store.into());
    rebuild_store_git_page(state);
    present_store_git_dialog(state);
}

pub fn show_store_git_page(state: &StoreGitPageState, store: impl Into<String>) {
    show_store_git_page_with_back_destination(state, store);
}

pub fn show_store_git_page_from_recipients(state: &StoreGitPageState, store: impl Into<String>) {
    show_store_git_page_with_back_destination(state, store);
}

pub fn rebuild_store_recipients_git_row(
    git_page: &StoreGitPageState,
    state: &StoreRecipientsPageState,
) {
    let refresh_id = next_store_recipients_git_row_refresh_id(&state.platform.git_list);
    state.clear_git_rows();
    if !supports_host_command_features() {
        state.platform.git_group.set_visible(false);
        return;
    }
    let Some(request) = state.current_request() else {
        state.platform.git_group.set_visible(false);
        return;
    };

    let visible = matches!(
        request.mode,
        StoreRecipientsMode::Edit | StoreRecipientsMode::Create
    );
    state.platform.git_group.set_visible(visible);
    if !visible {
        return;
    }

    let store = request.store.clone();
    let git_page_for_action = git_page.clone();
    let store_for_action = store.clone();
    let row = append_translated_action_row_with_button(
        &state.platform.git_list,
        &gettext("Git remotes"),
        &gettext("Loading"),
        "go-next-symbolic",
        move || {
            show_store_git_page_from_recipients(&git_page_for_action, store_for_action.clone());
        },
    );
    state.track_git_row(&row.clone().upcast());
    row.add_prefix(&dim_label_icon("git-symbolic"));
    row.set_sensitive(false);
    row.set_activatable(false);

    let store_for_worker = store.clone();
    let state_for_result = state.clone();
    let row_for_result = row.clone();
    let store_for_result = store.clone();
    let state_for_disconnect = state.clone();
    let row_for_disconnect = row;
    spawn_result_task(
        move || store_git_row_state_for_store(&store_for_worker),
        move |row_state| {
            if store_recipients_git_row_refresh_is_current(
                &state_for_result,
                refresh_id,
                &store_for_result,
            ) {
                apply_store_recipients_git_row_state(&row_for_result, &row_state);
                state_for_result.search.sync();
            }
        },
        move || {
            if store_recipients_git_row_refresh_is_current(
                &state_for_disconnect,
                refresh_id,
                &store,
            ) {
                apply_store_recipients_git_row_state(
                    &row_for_disconnect,
                    &store_git_row_state(Err("Git status worker disconnected".to_string())),
                );
                state_for_disconnect.search.sync();
            }
        },
    );
}

#[cfg(test)]
mod tests {
    use super::{
        remote_count_subtitle, store_git_row_state, StoreGitHead, StoreGitRepositoryStatus,
    };
    use crate::GitRemote;
    use keycord_runtime::i18n::gettext;

    #[test]
    fn git_row_is_disabled_when_git_state_cannot_be_inspected() {
        let state = store_git_row_state(Err("boom".to_string()));

        assert_eq!(state.subtitle, gettext("Couldn't inspect Git remotes."));
        assert!(!state.enabled);
    }

    #[test]
    fn git_row_stays_enabled_when_git_state_is_available() {
        let status = StoreGitRepositoryStatus {
            has_repository: true,
            head: StoreGitHead::Branch("main".to_string()),
            dirty: false,
            has_outgoing_commits: false,
            has_incoming_commits: false,
            remotes: vec![GitRemote {
                name: "origin".to_string(),
                url: "ssh://example.test/repo.git".to_string(),
            }],
        };

        let state = store_git_row_state(Ok(status.clone()));

        assert_eq!(
            remote_count_subtitle(&status),
            gettext("1 remote configured.")
        );
        assert!(state.enabled);
    }
}
