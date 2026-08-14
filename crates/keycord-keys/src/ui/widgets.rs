//! Keys-owned builder widgets and focus/navigation policy.

use adw::gtk::{Builder, ScrolledWindow, Stack};
use adw::prelude::*;
use adw::{ActionRow, EntryRow, NavigationPage, PasswordEntryRow, PreferencesGroup, StatusPage};
use keycord_shell::navigation::{NavigationPageId, NavigationPageRoute, PagePresentation};
use keycord_shell::ui::{
    connect_vertical_arrow_navigation_for_buttons, required_builder_object,
    visible_navigation_page_is, widget_contains_focus,
};

#[derive(Clone)]
pub struct KeyWindowWidgets {
    pub recipient_host_gpg_warning_group: PreferencesGroup,
    pub recipient_host_gpg_warning_row: ActionRow,
    pub recipient_keys_group: PreferencesGroup,
    pub recipient_create_group: PreferencesGroup,
    pub recipient_add_group: PreferencesGroup,
    pub generate_private_key_row: ActionRow,
    pub setup_hardware_key_row: ActionRow,
    pub add_hardware_key_row: ActionRow,
    pub import_hardware_key_row: ActionRow,
    pub import_clipboard_row: ActionRow,
    pub import_file_row: ActionRow,
    pub private_key_page: NavigationPage,
    pub private_key_stack: Stack,
    pub private_key_form: ScrolledWindow,
    pub private_key_loading: StatusPage,
    pub private_key_name_row: EntryRow,
    pub private_key_email_row: EntryRow,
    pub private_key_password_row: PasswordEntryRow,
    pub private_key_confirm_row: PasswordEntryRow,
    pub hardware_key_page: NavigationPage,
    pub hardware_key_stack: Stack,
    pub hardware_key_form: ScrolledWindow,
    pub hardware_key_loading: StatusPage,
    pub hardware_key_name_row: EntryRow,
    pub hardware_key_email_row: EntryRow,
    pub hardware_key_admin_pin_row: PasswordEntryRow,
    pub hardware_key_user_pin_row: PasswordEntryRow,
}

impl KeyWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        macro_rules! required {
            ($id:literal) => {
                required_builder_object(builder, $id)?
            };
        }

        Ok(Self {
            recipient_host_gpg_warning_group: required!("store_recipients_host_gpg_warning_group"),
            recipient_host_gpg_warning_row: required!("store_recipients_host_gpg_warning_row"),
            recipient_keys_group: required!("store_recipients_keys_group"),
            recipient_create_group: required!("store_recipients_create_group"),
            recipient_add_group: required!("store_recipients_add_group"),
            generate_private_key_row: required!("store_recipients_generate_key_row"),
            setup_hardware_key_row: required!("store_recipients_setup_hardware_key_row"),
            add_hardware_key_row: required!("store_recipients_add_hardware_key_row"),
            import_hardware_key_row: required!("store_recipients_import_hardware_key_row"),
            import_clipboard_row: required!("store_recipients_import_clipboard_row"),
            import_file_row: required!("store_recipients_import_file_row"),
            private_key_page: required!("private_key_generation_page"),
            private_key_stack: required!("private_key_generation_stack"),
            private_key_form: required!("private_key_generation_form"),
            private_key_loading: required!("private_key_generation_loading"),
            private_key_name_row: required!("private_key_generation_name_row"),
            private_key_email_row: required!("private_key_generation_email_row"),
            private_key_password_row: required!("private_key_generation_password_row"),
            private_key_confirm_row: required!("private_key_generation_confirm_row"),
            hardware_key_page: required!("hardware_key_generation_page"),
            hardware_key_stack: required!("hardware_key_generation_stack"),
            hardware_key_form: required!("hardware_key_generation_form"),
            hardware_key_loading: required!("hardware_key_generation_loading"),
            hardware_key_name_row: required!("hardware_key_generation_name_row"),
            hardware_key_email_row: required!("hardware_key_generation_email_row"),
            hardware_key_admin_pin_row: required!("hardware_key_generation_admin_pin_row"),
            hardware_key_user_pin_row: required!("hardware_key_generation_user_pin_row"),
        })
    }

    pub fn focus_private_key_target(&self) -> bool {
        self.private_key_name_row.grab_focus()
    }

    pub fn focus_hardware_key_target(&self) -> bool {
        self.hardware_key_name_row.grab_focus()
    }

    pub fn connect_keyboard_navigation(&self) {
        for page in [
            self.private_key_page.clone(),
            self.hardware_key_page.clone(),
        ] {
            connect_vertical_arrow_navigation_for_buttons(&page);
        }
    }

    pub fn focus_first_visible_page_target(
        &self,
        navigation: &adw::NavigationView,
    ) -> Option<bool> {
        if visible_navigation_page_is(navigation, &self.private_key_page) {
            return Some(self.focus_private_key_target());
        }
        if visible_navigation_page_is(navigation, &self.hardware_key_page) {
            return Some(self.focus_hardware_key_target());
        }
        None
    }

    pub fn visible_page_contains_focus(&self, navigation: &adw::NavigationView) -> Option<bool> {
        for page in [&self.private_key_page, &self.hardware_key_page] {
            if visible_navigation_page_is(navigation, page) {
                return Some(widget_contains_focus(&page.clone().upcast()));
            }
        }
        None
    }
}

pub fn private_key_generation_presentation() -> PagePresentation {
    PagePresentation::secondary(
        "Generate private key",
        "Create a password-protected private key for password stores.",
        false,
    )
}

pub fn hardware_key_generation_presentation() -> PagePresentation {
    PagePresentation::secondary(
        "Set up new hardware key (Experimental)",
        "Create an OpenPGP key on a blank connected smartcard or YubiKey.",
        false,
    )
}

pub const PRIVATE_KEY_GENERATION_PAGE_ID: NavigationPageId =
    NavigationPageId::new("private-key-generation");
pub const HARDWARE_KEY_GENERATION_PAGE_ID: NavigationPageId =
    NavigationPageId::new("hardware-key-generation");

pub fn key_generation_navigation_routes(widgets: &KeyWindowWidgets) -> [NavigationPageRoute; 2] {
    [
        NavigationPageRoute::secondary(
            PRIVATE_KEY_GENERATION_PAGE_ID,
            &widgets.private_key_page,
            private_key_generation_presentation(),
        ),
        NavigationPageRoute::secondary(
            HARDWARE_KEY_GENERATION_PAGE_ID,
            &widgets.hardware_key_page,
            hardware_key_generation_presentation(),
        ),
    ]
}
