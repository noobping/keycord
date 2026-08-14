use adw::glib;
use adw::gtk::{ListBox, ScrolledWindow, Spinner, Stack};
use adw::prelude::*;
use adw::StatusPage;
use keycord_runtime::i18n::gettext;
use keycord_shell::object_data::{cloned_data, set_cloned_data};

const PLACEHOLDER_STATE_KEY: &str = "password-list-placeholder-state";
const LOADING_TITLE: &str = "Loading";
const LOADING_DESCRIPTION: &str = "Loading your items.";

#[derive(Clone)]
pub(super) struct PasswordListPlaceholderState {
    stack: Stack,
    status: StatusPage,
    spinner: Spinner,
    list_view: ScrolledWindow,
    app_id: String,
}

impl PasswordListPlaceholderState {
    fn show_loading(&self) {
        self.show_status(PlaceholderPresentation {
            icon_name: &self.app_id,
            title: LOADING_TITLE,
            description: Some(LOADING_DESCRIPTION),
            spinner: true,
        });
    }

    fn show_resolved(&self, list: &ListBox, empty: bool, has_store_dirs: bool) {
        let presentation = if empty {
            if has_store_dirs {
                PlaceholderPresentation {
                    icon_name: &self.app_id,
                    title: "No items yet",
                    description: Some("Add an item to get started."),
                    spinner: false,
                }
            } else {
                PlaceholderPresentation {
                    icon_name: &self.app_id,
                    title: "No folders added",
                    description: Some("Open Preferences to add a store folder."),
                    spinner: false,
                }
            }
        } else {
            PlaceholderPresentation {
                icon_name: "edit-find-symbolic",
                title: "No matches",
                description: Some("Try another query or filter."),
                spinner: false,
            }
        };
        self.sync(list, presentation);
    }

    fn sync(&self, list: &ListBox, presentation: PlaceholderPresentation) {
        if has_visible_rows(list) {
            self.stack.set_visible_child(&self.list_view);
            return;
        }

        self.show_status(presentation);
    }

    fn show_status(&self, presentation: PlaceholderPresentation) {
        self.status.set_icon_name(Some(presentation.icon_name));
        self.status.set_title(&gettext(presentation.title));
        let description = presentation.description.map(gettext);
        self.status.set_description(description.as_deref());
        self.spinner.set_visible(presentation.spinner);
        self.spinner.set_spinning(presentation.spinner);
        self.stack.set_visible_child(&self.status);
    }
}

#[derive(Clone, Copy)]
struct PlaceholderPresentation<'a> {
    icon_name: &'a str,
    title: &'static str,
    description: Option<&'static str>,
    spinner: bool,
}

pub(super) fn register_placeholder_state(
    list: &ListBox,
    stack: &Stack,
    status: &StatusPage,
    spinner: &Spinner,
    list_view: &ScrolledWindow,
    app_id: &str,
) {
    set_cloned_data(
        list,
        PLACEHOLDER_STATE_KEY,
        PasswordListPlaceholderState {
            stack: stack.clone(),
            status: status.clone(),
            spinner: spinner.clone(),
            list_view: list_view.clone(),
            app_id: app_id.to_string(),
        },
    );
}

pub(super) fn show_loading_placeholder(list: &ListBox) {
    if let Some(state) = placeholder_state_for_list(list) {
        state.show_loading();
        return;
    }

    list.set_placeholder(Some(&loading_placeholder("io.github.noobping.keycord")));
}

pub(super) fn show_resolved_placeholder(list: &ListBox, empty: bool, has_store_dirs: bool) {
    if let Some(state) = placeholder_state_for_list(list) {
        let list = list.clone();
        glib::idle_add_local_once(move || {
            state.show_resolved(&list, empty, has_store_dirs);
        });
        return;
    }

    list.set_placeholder(Some(&resolved_placeholder(
        "io.github.noobping.keycord",
        empty,
        has_store_dirs,
    )));
}

fn placeholder_state_for_list(list: &ListBox) -> Option<PasswordListPlaceholderState> {
    cloned_data(list, PLACEHOLDER_STATE_KEY)
}

fn has_visible_rows(list: &ListBox) -> bool {
    let mut index = 0;
    while let Some(row) = list.row_at_index(index) {
        if row.is_child_visible() {
            return true;
        }
        index += 1;
    }

    false
}

fn loading_placeholder(app_id: &str) -> StatusPage {
    let spinner = Spinner::new();
    spinner.start();

    StatusPage::builder()
        .icon_name(app_id)
        .child(&spinner)
        .build()
}

fn resolved_placeholder(app_id: &str, empty: bool, has_store_dirs: bool) -> StatusPage {
    if empty {
        build_empty_password_list_placeholder(app_id, has_store_dirs)
    } else {
        StatusPage::builder()
            .icon_name("edit-find-symbolic")
            .title(gettext("No matches"))
            .description(gettext("Try another query or filter."))
            .build()
    }
}

fn build_empty_password_list_placeholder(symbolic: &str, has_store_dirs: bool) -> StatusPage {
    let builder = StatusPage::builder().icon_name(symbolic);
    if has_store_dirs {
        builder
            .title(gettext("No items yet"))
            .description(gettext("Add an item to get started."))
            .build()
    } else {
        builder
            .title(gettext("No folders added"))
            .description(gettext("Open Preferences to add a store folder."))
            .build()
    }
}
