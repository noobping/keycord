//! Preferences-owned builder widgets, focus policy, and navigation metadata.

use super::search::{PreferencesPageSearchState, SearchablePreferencesGroup};
use adw::glib::Propagation;
use adw::gtk::{
    gdk, Builder, Button, CheckButton, DirectionType, EventControllerKey, ListBox, SearchEntry,
    SpinButton, TextView, Widget,
};
use adw::prelude::*;
use adw::{
    ActionRow, ComboRow, EntryRow, NavigationPage, PreferencesGroup, PreferencesPage, ToastOverlay,
};
use keycord_shell::navigation::{PagePresentation, APP_WINDOW_TITLE};
use keycord_shell::ui::{
    configure_touch_friendly_search_entry, connect_horizontal_arrow_adjustment_for_spin_buttons,
    connect_ordered_list_arrow_navigation, connect_vertical_arrow_navigation_for_buttons,
    focus_first_matching_list_row_in_order, focus_first_visible_widget,
    focus_last_matching_list_row_in_order, focus_last_visible_widget,
    focused_row_is_last_matching_list_row, list_row_is_keyboard_focusable, required_builder_object,
    text_view_cursor_is_on_first_line, text_view_cursor_is_on_last_line,
    visible_navigation_page_is, widget_contains_focus,
};

#[derive(Clone)]
pub struct PreferencesWindowWidgets {
    pub page: NavigationPage,
    pub search_entry: SearchEntry,
    pub preferences_page: PreferencesPage,
    pub search_empty_group: PreferencesGroup,
    pub store_list_group: PreferencesGroup,
    pub store_actions_group: PreferencesGroup,
    pub username_group: PreferencesGroup,
    pub password_list_group: PreferencesGroup,
    pub template_group: PreferencesGroup,
    pub clear_empty_fields_group: PreferencesGroup,
    pub generator_group: PreferencesGroup,
    pub template_view: TextView,
    pub clear_empty_fields_row: ActionRow,
    pub clear_empty_fields_check: CheckButton,
    pub username_folder_check: CheckButton,
    pub username_filename_check: CheckButton,
    pub password_list_sort_filename_check: CheckButton,
    pub password_list_sort_hybrid_check: CheckButton,
    pub password_list_sort_store_path_check: CheckButton,
    pub stores: ListBox,
    pub store_actions: ListBox,
    pub generator_length_spin: SpinButton,
    pub generator_min_lowercase_spin: SpinButton,
    pub generator_min_uppercase_spin: SpinButton,
    pub generator_min_numbers_spin: SpinButton,
    pub generator_min_symbols_spin: SpinButton,
    pub backend_group: PreferencesGroup,
    pub host_access_group: PreferencesGroup,
    pub backend_row: ComboRow,
    pub pass_command_row: EntryRow,
    pub sync_private_keys_row: ActionRow,
    pub sync_private_keys_check: CheckButton,
    pub audit_history_recipients_row: ActionRow,
    pub audit_history_recipients_check: CheckButton,
    pub username_filename_row: ActionRow,
    pub username_folder_row: ActionRow,
    pub password_list_sort_filename_row: ActionRow,
    pub password_list_sort_hybrid_row: ActionRow,
    pub password_list_sort_store_path_row: ActionRow,
    pub generator_length_row: ActionRow,
    pub generator_min_lowercase_row: ActionRow,
    pub generator_min_uppercase_row: ActionRow,
    pub generator_min_numbers_row: ActionRow,
    pub generator_min_symbols_row: ActionRow,
}

impl PreferencesWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        macro_rules! required {
            ($id:literal) => {
                required_builder_object(builder, $id)?
            };
        }

        Ok(Self {
            page: required!("settings_page"),
            search_entry: required!("settings_search_entry"),
            preferences_page: required!("settings_preferences_page"),
            search_empty_group: required!("settings_search_empty_group"),
            store_list_group: required!("settings_store_list_group"),
            store_actions_group: required!("settings_store_actions_group"),
            username_group: required!("settings_username_group"),
            password_list_group: required!("settings_password_list_group"),
            template_group: required!("settings_template_group"),
            clear_empty_fields_group: required!("settings_clear_empty_fields_group"),
            generator_group: required!("settings_generator_group"),
            template_view: required!("new_pass_file_template_view"),
            clear_empty_fields_row: required!("clear_empty_fields_before_save_row"),
            clear_empty_fields_check: required!("clear_empty_fields_before_save_check"),
            username_folder_check: required!("preferences_username_folder_check"),
            username_filename_check: required!("preferences_username_filename_check"),
            password_list_sort_filename_check: required!(
                "preferences_password_list_sort_filename_check"
            ),
            password_list_sort_hybrid_check: required!(
                "preferences_password_list_sort_hybrid_check"
            ),
            password_list_sort_store_path_check: required!(
                "preferences_password_list_sort_store_path_check"
            ),
            stores: required!("password_stores"),
            store_actions: required!("password_store_actions"),
            generator_length_spin: required!("preferences_password_generator_length_spin"),
            generator_min_lowercase_spin: required!(
                "preferences_password_generator_min_lowercase_spin"
            ),
            generator_min_uppercase_spin: required!(
                "preferences_password_generator_min_uppercase_spin"
            ),
            generator_min_numbers_spin: required!(
                "preferences_password_generator_min_numbers_spin"
            ),
            generator_min_symbols_spin: required!(
                "preferences_password_generator_min_symbols_spin"
            ),
            backend_group: required!("backend_preferences"),
            host_access_group: required!("host_access_preferences_group"),
            backend_row: required!("backend_row"),
            pass_command_row: required!("pass_command_row"),
            sync_private_keys_row: required!("sync_private_keys_with_host_row"),
            sync_private_keys_check: required!("sync_private_keys_with_host_check"),
            audit_history_recipients_row: required!("audit_use_commit_history_recipients_row"),
            audit_history_recipients_check: required!("audit_use_commit_history_recipients_check"),
            username_filename_row: required!("preferences_username_filename_row"),
            username_folder_row: required!("preferences_username_folder_row"),
            password_list_sort_filename_row: required!(
                "preferences_password_list_sort_filename_row"
            ),
            password_list_sort_hybrid_row: required!("preferences_password_list_sort_hybrid_row"),
            password_list_sort_store_path_row: required!(
                "preferences_password_list_sort_store_path_row"
            ),
            generator_length_row: required!("preferences_password_generator_length_row"),
            generator_min_lowercase_row: required!(
                "preferences_password_generator_min_lowercase_row"
            ),
            generator_min_uppercase_row: required!(
                "preferences_password_generator_min_uppercase_row"
            ),
            generator_min_numbers_row: required!("preferences_password_generator_min_numbers_row"),
            generator_min_symbols_row: required!("preferences_password_generator_min_symbols_row"),
        })
    }

    /// Build the Preferences-owned search projection for its page.
    pub fn search_state(&self) -> PreferencesPageSearchState {
        PreferencesPageSearchState::new(
            &self.preferences_page,
            &self.search_entry,
            Some(&self.search_empty_group),
            vec![
                SearchablePreferencesGroup::with_list_box(&self.store_list_group, &self.stores),
                SearchablePreferencesGroup::with_list_box(
                    &self.store_actions_group,
                    &self.store_actions,
                ),
                SearchablePreferencesGroup::with_widgets(
                    &self.backend_group,
                    vec![
                        self.backend_row.clone().upcast(),
                        self.pass_command_row.clone().upcast(),
                        self.sync_private_keys_row.clone().upcast(),
                        self.audit_history_recipients_row.clone().upcast(),
                    ],
                ),
                SearchablePreferencesGroup::with_widgets(&self.host_access_group, Vec::new()),
                SearchablePreferencesGroup::with_widgets(
                    &self.username_group,
                    vec![
                        self.username_filename_row.clone().upcast(),
                        self.username_folder_row.clone().upcast(),
                    ],
                ),
                SearchablePreferencesGroup::with_widgets(
                    &self.password_list_group,
                    vec![
                        self.password_list_sort_filename_row.clone().upcast(),
                        self.password_list_sort_hybrid_row.clone().upcast(),
                        self.password_list_sort_store_path_row.clone().upcast(),
                    ],
                ),
                SearchablePreferencesGroup::with_widgets(&self.template_group, Vec::new()),
                SearchablePreferencesGroup::with_widgets(
                    &self.clear_empty_fields_group,
                    vec![self.clear_empty_fields_row.clone().upcast()],
                ),
                SearchablePreferencesGroup::with_widgets(
                    &self.generator_group,
                    vec![
                        self.generator_length_row.clone().upcast(),
                        self.generator_min_lowercase_row.clone().upcast(),
                        self.generator_min_uppercase_row.clone().upcast(),
                        self.generator_min_numbers_row.clone().upcast(),
                        self.generator_min_symbols_row.clone().upcast(),
                    ],
                ),
            ],
        )
    }

    /// Map Preferences-owned controls while allowing composition to supply a compatible
    /// password-generation adapter without introducing a subject dependency.
    pub fn page_controls<C>(
        &self,
        generator_controls: C,
        overlay: &ToastOverlay,
    ) -> super::settings::PreferencesPageControls<C> {
        super::settings::PreferencesPageControls {
            search: self.search_state(),
            template_view: self.template_view.clone(),
            clear_empty_fields_before_save_row: self.clear_empty_fields_row.clone(),
            clear_empty_fields_before_save_check: self.clear_empty_fields_check.clone(),
            username_folder_check: self.username_folder_check.clone(),
            username_filename_check: self.username_filename_check.clone(),
            password_list_sort_filename_check: self.password_list_sort_filename_check.clone(),
            password_list_sort_hybrid_check: self.password_list_sort_hybrid_check.clone(),
            password_list_sort_store_path_check: self.password_list_sort_store_path_check.clone(),
            generator_controls,
            overlay: overlay.clone(),
            pass_row: self.pass_command_row.clone(),
            backend_row: self.backend_row.clone(),
            sync_private_keys_row: self.sync_private_keys_row.clone(),
            sync_private_keys_check: self.sync_private_keys_check.clone(),
            audit_use_commit_history_recipients_row: self.audit_history_recipients_row.clone(),
            audit_use_commit_history_recipients_check: self.audit_history_recipients_check.clone(),
        }
    }

    /// Apply availability of host-command settings to Preferences-owned groups.
    pub fn set_host_features_available(&self, available: bool) {
        self.backend_group.set_visible(available);
        if !available {
            self.host_access_group.set_visible(false);
        }
    }

    /// Let composition adapt the Preferences-owned generation controls without knowing IDs.
    pub fn map_password_generation_controls<C>(
        &self,
        build: impl FnOnce(&SpinButton, &SpinButton, &SpinButton, &SpinButton, &SpinButton) -> C,
    ) -> C {
        build(
            &self.generator_length_spin,
            &self.generator_min_lowercase_spin,
            &self.generator_min_uppercase_spin,
            &self.generator_min_numbers_spin,
            &self.generator_min_symbols_spin,
        )
    }

    fn lists(&self) -> [ListBox; 2] {
        [self.stores.clone(), self.store_actions.clone()]
    }

    fn detail_widgets(&self) -> Vec<Widget> {
        vec![
            self.backend_row.clone().upcast(),
            self.pass_command_row.clone().upcast(),
            self.sync_private_keys_check.clone().upcast(),
            self.audit_history_recipients_check.clone().upcast(),
            self.username_filename_check.clone().upcast(),
            self.username_folder_check.clone().upcast(),
            self.password_list_sort_filename_check.clone().upcast(),
            self.password_list_sort_hybrid_check.clone().upcast(),
            self.password_list_sort_store_path_check.clone().upcast(),
            self.template_view.clone().upcast(),
            self.clear_empty_fields_check.clone().upcast(),
            self.generator_length_spin.clone().upcast(),
            self.generator_min_lowercase_spin.clone().upcast(),
            self.generator_min_uppercase_spin.clone().upcast(),
            self.generator_min_numbers_spin.clone().upcast(),
            self.generator_min_symbols_spin.clone().upcast(),
        ]
    }

    fn focus_first_detail(&self) -> bool {
        focus_first_visible_widget(&self.detail_widgets())
    }

    pub fn connect_keyboard_navigation(&self, primary_menu_button: &Widget) {
        connect_vertical_arrow_navigation_for_buttons(&self.page);
        connect_horizontal_arrow_adjustment_for_spin_buttons(&self.page);
        connect_ordered_list_arrow_navigation(
            &self.lists(),
            Some(primary_menu_button),
            list_row_is_keyboard_focusable,
        );

        let actions_list = self.store_actions.clone();
        let widgets_for_down = self.clone();
        let down_controller = EventControllerKey::new();
        down_controller.set_propagation_phase(adw::gtk::PropagationPhase::Capture);
        down_controller.connect_key_pressed(move |_, key, _, _| {
            if !matches!(key, gdk::Key::Down | gdk::Key::KP_Down)
                || !focused_row_is_last_matching_list_row(
                    &actions_list,
                    list_row_is_keyboard_focusable,
                )
            {
                return Propagation::Proceed;
            }
            if widgets_for_down.focus_first_detail() {
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        });
        self.store_actions.add_controller(down_controller);

        let widgets_for_details = self.clone();
        let details_controller = EventControllerKey::new();
        details_controller.set_propagation_phase(adw::gtk::PropagationPhase::Capture);
        details_controller.connect_key_pressed(move |_, key, _, _| {
            let direction = match key {
                gdk::Key::Up | gdk::Key::KP_Up => DirectionType::Up,
                gdk::Key::Down | gdk::Key::KP_Down => DirectionType::Down,
                _ => return Propagation::Proceed,
            };
            let details = widgets_for_details.detail_widgets();
            let Some(current_index) = details.iter().position(widget_contains_focus) else {
                return Propagation::Proceed;
            };
            if widget_contains_focus(&widgets_for_details.template_view.clone().upcast())
                && ((matches!(direction, DirectionType::Up)
                    && !text_view_cursor_is_on_first_line(&widgets_for_details.template_view))
                    || (matches!(direction, DirectionType::Down)
                        && !text_view_cursor_is_on_last_line(&widgets_for_details.template_view)))
            {
                return Propagation::Proceed;
            }

            let moved = match direction {
                DirectionType::Up if current_index == 0 => focus_last_matching_list_row_in_order(
                    &widgets_for_details.lists(),
                    list_row_is_keyboard_focusable,
                ),
                DirectionType::Up => focus_last_visible_widget(&details[..current_index]),
                DirectionType::Down => focus_first_visible_widget(&details[current_index + 1..]),
                _ => false,
            };
            if moved {
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        });
        self.page.add_controller(details_controller);
    }

    pub fn configure_search_entries(&self) {
        configure_touch_friendly_search_entry(&self.search_entry);
    }

    pub fn toggle_find_for_visible_page(
        &self,
        navigation: &adw::NavigationView,
        find_button: &Button,
    ) -> bool {
        if !visible_navigation_page_is(navigation, &self.page) {
            return false;
        }
        keycord_shell::ui::toggle_page_search_entry(find_button, &self.search_entry);
        true
    }

    pub fn focus_first_target(&self) -> bool {
        if self.search_entry.is_visible() {
            return self.search_entry.grab_focus();
        }
        focus_first_matching_list_row_in_order(&self.lists(), list_row_is_keyboard_focusable)
            || self.focus_first_detail()
            || self.page.child_focus(DirectionType::Down)
    }

    pub fn focus_first_visible_page_target(
        &self,
        navigation: &adw::NavigationView,
    ) -> Option<bool> {
        visible_navigation_page_is(navigation, &self.page).then(|| self.focus_first_target())
    }

    pub fn visible_page_contains_focus(&self, navigation: &adw::NavigationView) -> Option<bool> {
        visible_navigation_page_is(navigation, &self.page)
            .then(|| widget_contains_focus(&self.page.clone().upcast()))
    }
}

pub fn preferences_page_presentation() -> PagePresentation {
    PagePresentation::secondary("Preferences", APP_WINDOW_TITLE, false).with_find_visible(true)
}

pub fn configure_preferences_shortcuts(app: &adw::Application) {
    app.set_accels_for_action("win.open-preferences", &["<primary>comma"]);
}
