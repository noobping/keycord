//! Stores-owned builder widgets, focus policy, and shortcut metadata.

use adw::gtk::{
    Box as GtkBox, Builder, Button, CheckButton, ScrolledWindow, SearchEntry, Stack, Widget,
};
use adw::prelude::*;
use adw::{
    ActionRow, ComboRow, EntryRow, NavigationPage, PasswordEntryRow, PreferencesGroup,
    PreferencesPage, StatusPage, ToastOverlay,
};
use keycord_preferences::ui::{PreferencesPageSearchState, SearchablePreferencesGroup};
use keycord_shell::navigation::PagePresentation;
use keycord_shell::ui::{
    configure_touch_friendly_search_entry, connect_vertical_arrow_navigation_for_buttons,
    required_builder_object, visible_navigation_page_is, widget_contains_focus,
};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct StoresWindowWidgets {
    pub store_button: Button,
    pub import_page: NavigationPage,
    pub import_stack: Stack,
    pub import_form: ScrolledWindow,
    pub import_loading: StatusPage,
    pub import_store_dropdown: ComboRow,
    pub import_source_dropdown: ComboRow,
    pub import_source_path_row: ActionRow,
    pub import_source_file_button: Button,
    pub import_source_folder_button: Button,
    pub import_source_clear_button: Button,
    pub import_password_row: PasswordEntryRow,
    pub import_target_path_row: EntryRow,
    pub import_button: Button,
    pub recipients_page: NavigationPage,
    pub recipients_stack: Stack,
    pub recipients_content: GtkBox,
    pub recipients_loading: StatusPage,
    pub recipients_search_entry: SearchEntry,
    pub recipients_preferences_page: PreferencesPage,
    pub recipients_back_row: ActionRow,
    pub recipients_search_empty_group: PreferencesGroup,
    pub recipients_scope_group: PreferencesGroup,
    pub recipients_saving_group: PreferencesGroup,
    pub recipients_options_group: PreferencesGroup,
    pub recipients_scope_row: ComboRow,
    pub recipients_git_group: PreferencesGroup,
    pub recipients_require_all_row: ActionRow,
    pub recipients_require_all_check: CheckButton,
}

impl StoresWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        macro_rules! required {
            ($id:literal) => {
                required_builder_object(builder, $id)?
            };
        }

        Ok(Self {
            store_button: required!("store_button"),
            import_page: required!("store_import_page"),
            import_stack: required!("store_import_stack"),
            import_form: required!("store_import_form"),
            import_loading: required!("store_import_loading"),
            import_store_dropdown: required!("store_import_store_dropdown"),
            import_source_dropdown: required!("store_import_source_dropdown"),
            import_source_path_row: required!("store_import_source_path_row"),
            import_source_file_button: required!("store_import_source_file_button"),
            import_source_folder_button: required!("store_import_source_folder_button"),
            import_source_clear_button: required!("store_import_source_clear_button"),
            import_password_row: required!("store_import_password_row"),
            import_target_path_row: required!("store_import_target_path_row"),
            import_button: required!("store_import_button"),
            recipients_page: required!("store_recipients_page"),
            recipients_stack: required!("store_recipients_stack"),
            recipients_content: required!("store_recipients_content"),
            recipients_loading: required!("store_recipients_loading"),
            recipients_search_entry: required!("store_recipients_search_entry"),
            recipients_preferences_page: required!("store_recipients_preferences_page"),
            recipients_back_row: required!("store_recipients_back_row"),
            recipients_search_empty_group: required!("store_recipients_search_empty_group"),
            recipients_scope_group: required!("store_recipients_scope_group"),
            recipients_saving_group: required!("store_recipients_saving_group"),
            recipients_options_group: required!("store_recipients_options_group"),
            recipients_scope_row: required!("store_recipients_scope_row"),
            recipients_git_group: required!("store_recipients_git_group"),
            recipients_require_all_row: required!("store_recipients_require_all_row"),
            recipients_require_all_check: required!("store_recipients_require_all_check"),
        })
    }

    /// Build the Stores-owned platform projection for the recipient workflow.
    pub fn recipient_platform_state(
        &self,
        overlay: &ToastOverlay,
    ) -> super::recipient_page::StoreRecipientsPlatformState {
        super::recipient_page::StoreRecipientsPlatformState {
            overlay: overlay.clone(),
            recipients_stack: self.recipients_stack.clone(),
            recipients_content: self.recipients_content.clone(),
            recipients_loading: self.recipients_loading.clone(),
            scope_group: self.recipients_scope_group.clone(),
            saving_group: self.recipients_saving_group.clone(),
            scope_list: self.recipients_scope_group.clone(),
            options_group: self.recipients_options_group.clone(),
            options_list: self.recipients_options_group.clone(),
            scope_row: self.recipients_scope_row.clone(),
            git_group: self.recipients_git_group.clone(),
            git_list: self.recipients_git_group.clone(),
            require_all_row: self.recipients_require_all_row.clone(),
            require_all_check: self.recipients_require_all_check.clone(),
        }
    }

    /// Build the Stores-owned search projection and include reviewed subject contributions.
    pub fn recipient_search_state(
        &self,
        subject_groups: impl IntoIterator<Item = SearchablePreferencesGroup>,
        git_rows: Rc<RefCell<Vec<Widget>>>,
    ) -> PreferencesPageSearchState {
        let mut groups = vec![SearchablePreferencesGroup::with_widgets(
            &self.recipients_scope_group,
            vec![self.recipients_scope_row.clone().upcast()],
        )];
        groups.extend(subject_groups);
        groups.extend([
            SearchablePreferencesGroup::with_widgets(
                &self.recipients_options_group,
                vec![self.recipients_require_all_row.clone().upcast()],
            ),
            SearchablePreferencesGroup::with_tracked_widgets(&self.recipients_git_group, git_rows),
        ]);
        PreferencesPageSearchState::new(
            &self.recipients_preferences_page,
            &self.recipients_search_entry,
            Some(&self.recipients_search_empty_group),
            groups,
        )
    }

    pub fn focus_import_target(&self) -> bool {
        self.import_store_dropdown.grab_focus()
    }

    pub fn focus_recipients_target(&self) -> bool {
        if self.recipients_search_entry.is_visible() {
            self.recipients_search_entry.grab_focus()
        } else {
            self.recipients_page
                .child_focus(adw::gtk::DirectionType::Down)
        }
    }

    pub fn connect_keyboard_navigation(&self) {
        for page in [self.import_page.clone(), self.recipients_page.clone()] {
            connect_vertical_arrow_navigation_for_buttons(&page);
        }
    }

    pub fn configure_search_entries(&self) {
        configure_touch_friendly_search_entry(&self.recipients_search_entry);
    }

    pub fn toggle_find_for_visible_page(
        &self,
        navigation: &adw::NavigationView,
        find_button: &Button,
    ) -> bool {
        if visible_navigation_page_is(navigation, &self.recipients_page) {
            keycord_shell::ui::toggle_page_search_entry(find_button, &self.recipients_search_entry);
            return true;
        }
        visible_navigation_page_is(navigation, &self.import_page)
    }

    pub fn focus_first_visible_page_target(
        &self,
        navigation: &adw::NavigationView,
    ) -> Option<bool> {
        if visible_navigation_page_is(navigation, &self.import_page) {
            return Some(self.focus_import_target());
        }
        if visible_navigation_page_is(navigation, &self.recipients_page) {
            return Some(self.focus_recipients_target());
        }
        None
    }

    pub fn visible_page_contains_focus(&self, navigation: &adw::NavigationView) -> Option<bool> {
        for page in [&self.import_page, &self.recipients_page] {
            if visible_navigation_page_is(navigation, page) {
                return Some(widget_contains_focus(&page.clone().upcast()));
            }
        }
        None
    }
}

pub fn store_import_page_presentation() -> PagePresentation {
    PagePresentation::secondary(
        "Import passwords",
        "Use pass import to import into an existing store.",
        false,
    )
}

pub fn configure_store_shortcuts(app: &adw::Application) {
    app.set_accels_for_action("win.open-store-picker", &["<primary><shift>n"]);
    for slot in 1..=super::management::NUMBERED_STORE_SHORTCUT_COUNT {
        let action = format!("win.open-store-recipients-{slot}");
        let accelerator = format!("<primary>{slot}");
        app.set_accels_for_action(&action, &[accelerator.as_str()]);
    }
}
