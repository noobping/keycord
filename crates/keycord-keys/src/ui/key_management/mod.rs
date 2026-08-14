//! Keys-owned recipient-page actions and private-key import/generation lifecycle.

mod form;
mod hardware;
mod key_sync;
mod private;
mod recipient_list;

use super::KeyWindowWidgets;
use crate::{
    hardware_key_available, smartcard_available, DiscoveredHardwareToken, HostGpgPrivateKeySummary,
};
use adw::gtk::{Button, Widget};
use adw::prelude::*;
use adw::{ActionRow, ApplicationWindow, NavigationView, PreferencesGroup, ToastOverlay};
use keycord_preferences::ui::SearchablePreferencesGroup;
use keycord_shell::ui::visible_navigation_page_is;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

pub use recipient_list::{
    BeforeRecipientKeyRows, RecipientKeyChoiceVisibility, RecipientKeyDeleteMessage,
    RecipientKeyListContext, RecipientKeyListPolicy, RecipientKeyMatcher, RecipientKeyToggle,
    RecipientKeyToggleMessage,
};

pub type ReadKeyTextResult = Rc<dyn Fn(Result<Option<String>, String>)>;
pub type ReadKeyText = Rc<dyn Fn(ReadKeyTextResult)>;
pub type SyncOptionalKeyAccess = Rc<dyn Fn(&PreferencesGroup, &ToastOverlay, &[&ActionRow], bool)>;
pub type ListHostPrivateKeys =
    Arc<dyn Fn() -> Result<Vec<HostGpgPrivateKeySummary>, String> + Send + Sync>;
pub type CopyKeyText = Rc<dyn Fn(&str, &ToastOverlay, Option<&Button>) -> bool>;
pub type PromptKeyUnlock = Rc<dyn Fn(&ToastOverlay, String, Rc<dyn Fn()>, Rc<dyn Fn(bool)>)>;
pub type IsKeyNoticeHidden = Rc<dyn Fn(&str) -> bool>;
pub type HideKeyNotice = Rc<dyn Fn(&str) -> Result<(), String>>;
pub type KeySyncEnabled = Rc<dyn Fn() -> bool>;
pub type DisableKeySync = Rc<dyn Fn() -> Result<(), String>>;
pub type SyncPrivateKeys = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
pub type RefreshKeyConsumers = Rc<dyn Fn(&ApplicationWindow)>;

/// Application infrastructure used by the Keys-owned controller.
#[derive(Clone)]
pub struct KeyManagementUiPorts {
    pub read_clipboard_text: ReadKeyText,
    pub list_host_private_keys: ListHostPrivateKeys,
    pub copy_text: CopyKeyText,
    pub prompt_unlock: PromptKeyUnlock,
    pub is_notice_hidden: IsKeyNoticeHidden,
    pub hide_notice: HideKeyNotice,
    pub private_key_sync_enabled: KeySyncEnabled,
    pub disable_private_key_sync: DisableKeySync,
    pub sync_private_keys_from_host: SyncPrivateKeys,
    pub sync_private_keys_to_host: SyncPrivateKeys,
    pub refresh_key_consumers: RefreshKeyConsumers,
    pub sync_optional_smartcard_access: SyncOptionalKeyAccess,
    #[cfg(feature = "fido-ui")]
    pub sync_optional_fido_access: SyncOptionalKeyAccess,
}

/// Store-recipient policy and completion callbacks consumed by Keys.
///
/// Keys owns every key operation and its presentation. The embedding recipient
/// workflow only decides whether standard-key actions are currently allowed and
/// refreshes its own projections after the managed key set changes.
#[derive(Clone)]
pub struct KeyRecipientWorkflowPorts {
    pub standard_actions_allowed: Rc<dyn Fn() -> bool>,
    pub on_key_changed: Rc<dyn Fn()>,
    pub on_key_access_changed: Rc<dyn Fn()>,
    pub on_generation_page_closed: Rc<dyn Fn(bool)>,
}

pub struct KeyManagementUiParts {
    pub window: ApplicationWindow,
    pub navigation: NavigationView,
    pub overlay: ToastOverlay,
    pub widgets: KeyWindowWidgets,
    #[cfg(feature = "fido-ui")]
    pub fido: keycord_fido::ui::FidoWindowWidgets,
    pub ports: KeyManagementUiPorts,
}

#[derive(Clone)]
pub struct KeyManagementUiState {
    pub(crate) window: ApplicationWindow,
    pub(crate) navigation: NavigationView,
    pub(crate) overlay: ToastOverlay,
    pub(crate) widgets: KeyWindowWidgets,
    #[cfg(feature = "fido-ui")]
    pub(crate) fido: keycord_fido::ui::FidoWindowWidgets,
    pub(crate) ports: KeyManagementUiPorts,
    pub(crate) hardware_generation_token: Rc<RefCell<Option<DiscoveredHardwareToken>>>,
    pub(crate) private_generation_in_flight: Rc<Cell<bool>>,
    pub(crate) hardware_generation_in_flight: Rc<Cell<bool>>,
    pub(crate) recipient_rows: Rc<RefCell<Vec<Widget>>>,
    reopen_recipient_page: Rc<Cell<bool>>,
    workflow: Rc<RefCell<Option<KeyRecipientWorkflowPorts>>>,
    controls_connected: Rc<Cell<bool>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RecipientActionPresentation {
    generate_private_key: bool,
    import_private_key: bool,
    setup_hardware_key: bool,
    connect_hardware_key: bool,
}

impl RecipientActionPresentation {
    const fn new(
        standard_actions_enabled: bool,
        hardware_generation_supported: bool,
        smartcard_supported: bool,
    ) -> Self {
        Self {
            generate_private_key: standard_actions_enabled,
            import_private_key: standard_actions_enabled,
            setup_hardware_key: standard_actions_enabled && hardware_generation_supported,
            connect_hardware_key: standard_actions_enabled && smartcard_supported,
        }
    }

    const fn create_group_visible(self) -> bool {
        self.generate_private_key
    }

    const fn hardware_rows_visible(self) -> bool {
        self.setup_hardware_key || self.connect_hardware_key
    }

    const fn add_group_visible(self, fido_generation_visible: bool) -> bool {
        fido_generation_visible || self.hardware_rows_visible() || self.import_private_key
    }
}

impl KeyManagementUiState {
    pub fn new(parts: KeyManagementUiParts) -> Self {
        let state = Self {
            window: parts.window,
            navigation: parts.navigation,
            overlay: parts.overlay,
            widgets: parts.widgets,
            #[cfg(feature = "fido-ui")]
            fido: parts.fido,
            ports: parts.ports,
            hardware_generation_token: Rc::new(RefCell::new(None)),
            private_generation_in_flight: Rc::new(Cell::new(false)),
            hardware_generation_in_flight: Rc::new(Cell::new(false)),
            recipient_rows: Rc::new(RefCell::new(Vec::new())),
            reopen_recipient_page: Rc::new(Cell::new(false)),
            workflow: Rc::new(RefCell::new(None)),
            controls_connected: Rc::new(Cell::new(false)),
        };
        recipient_list::connect_recipient_warning_control(&state);
        state
    }

    /// Connect the Keys-owned actions to the embedding recipient workflow.
    pub fn connect_recipient_controls(&self, workflow: KeyRecipientWorkflowPorts) {
        self.workflow.replace(Some(workflow));
        if self.controls_connected.replace(true) {
            return;
        }

        private::connect_controls(self);
        hardware::connect_controls(self);
    }

    /// Apply store selection policy while Keys retains capability and access UI.
    pub fn sync_recipient_action_visibility(
        &self,
        standard_actions_enabled: bool,
        uses_integrated_backend: bool,
    ) {
        let presentation = RecipientActionPresentation::new(
            standard_actions_enabled,
            hardware_key_available(),
            smartcard_available(),
        );
        self.widgets
            .generate_private_key_row
            .set_visible(presentation.generate_private_key);
        #[cfg(feature = "fido-ui")]
        let fido_generation_visible = self
            .fido
            .sync_generation_visibility(standard_actions_enabled);
        #[cfg(not(feature = "fido-ui"))]
        let fido_generation_visible = false;
        self.widgets
            .import_clipboard_row
            .set_visible(presentation.import_private_key);
        self.widgets
            .import_file_row
            .set_visible(presentation.import_private_key);
        self.widgets
            .setup_hardware_key_row
            .set_visible(presentation.setup_hardware_key);
        self.widgets
            .add_hardware_key_row
            .set_visible(presentation.connect_hardware_key);
        self.widgets
            .import_hardware_key_row
            .set_visible(presentation.connect_hardware_key);

        let hardware_rows = [
            &self.widgets.setup_hardware_key_row,
            &self.widgets.add_hardware_key_row,
            &self.widgets.import_hardware_key_row,
        ];
        (self.ports.sync_optional_smartcard_access)(
            &self.widgets.recipient_add_group,
            &self.overlay,
            &hardware_rows,
            presentation.hardware_rows_visible(),
        );
        #[cfg(feature = "fido-ui")]
        (self.ports.sync_optional_fido_access)(
            &self.widgets.recipient_add_group,
            &self.overlay,
            &[self.fido.generation_row()],
            uses_integrated_backend && fido_generation_visible,
        );

        #[cfg(not(feature = "fido-ui"))]
        let _ = uses_integrated_backend;

        self.widgets
            .recipient_create_group
            .set_visible(presentation.create_group_visible());
        self.widgets
            .recipient_add_group
            .set_visible(presentation.add_group_visible(fido_generation_visible));
    }

    pub fn handle_generation_subpage_back(&self) -> bool {
        if !visible_navigation_page_is(&self.navigation, &self.widgets.private_key_page)
            && !visible_navigation_page_is(&self.navigation, &self.widgets.hardware_key_page)
        {
            return false;
        }

        self.navigation.pop();
        self.notify_generation_page_closed();
        true
    }

    pub fn reset_recipient_navigation(&self) {
        self.reopen_recipient_page.set(false);
    }

    pub fn widgets(&self) -> &KeyWindowWidgets {
        &self.widgets
    }

    pub fn rebuild_recipient_key_list(&self, context: RecipientKeyListContext) {
        recipient_list::rebuild_recipient_key_list(self, context);
    }

    /// Search groups contributed by the Keys-owned portion of the recipient page.
    pub fn recipient_search_groups(&self) -> [SearchablePreferencesGroup; 4] {
        let add_key_widgets = vec![
            self.widgets.setup_hardware_key_row.clone().upcast(),
            self.widgets.add_hardware_key_row.clone().upcast(),
            self.widgets.import_hardware_key_row.clone().upcast(),
            self.widgets.import_clipboard_row.clone().upcast(),
            self.widgets.import_file_row.clone().upcast(),
        ];
        #[cfg(feature = "fido-ui")]
        let add_key_widgets = {
            let mut widgets = self.fido.recipient_search_widgets();
            widgets.extend(add_key_widgets);
            widgets
        };

        [
            SearchablePreferencesGroup::with_widgets(
                &self.widgets.recipient_host_gpg_warning_group,
                vec![self.widgets.recipient_host_gpg_warning_row.clone().upcast()],
            ),
            SearchablePreferencesGroup::with_tracked_widgets(
                &self.widgets.recipient_keys_group,
                self.recipient_rows.clone(),
            ),
            SearchablePreferencesGroup::with_widgets(
                &self.widgets.recipient_create_group,
                vec![self.widgets.generate_private_key_row.clone().upcast()],
            ),
            SearchablePreferencesGroup::with_widgets(
                &self.widgets.recipient_add_group,
                add_key_widgets,
            ),
        ]
    }

    pub fn append_recipient_projection_row(&self, row: &adw::ActionRow) {
        recipient_list::append_recipient_projection_row(self, row);
    }

    pub fn sync_recipient_group_header(&self, scope_selector_visible: bool) {
        recipient_list::sync_recipient_group_header(self, scope_selector_visible);
    }

    pub fn request_recipient_key_access(
        &self,
        fingerprint: String,
        after_unlock: Rc<dyn Fn()>,
        on_finish: Rc<dyn Fn(bool)>,
    ) {
        (self.ports.prompt_unlock)(&self.overlay, fingerprint, after_unlock, on_finish);
    }

    pub fn refresh_recipient_key_inventory(&self) -> bool {
        let outcome = key_sync::sync_from_host(self);
        if matches!(outcome, key_sync::KeySyncOutcome::Succeeded) {
            (self.ports.refresh_key_consumers)(&self.window);
        }
        !matches!(outcome, key_sync::KeySyncOutcome::Failed)
    }

    pub(crate) fn standard_actions_allowed(&self) -> bool {
        self.workflow
            .borrow()
            .as_ref()
            .is_some_and(|ports| (ports.standard_actions_allowed)())
    }

    pub(crate) fn notify_key_changed(&self) {
        let _ = key_sync::sync_to_host(self);
        (self.ports.refresh_key_consumers)(&self.window);
        if let Some(ports) = self.workflow.borrow().as_ref() {
            (ports.on_key_changed)();
        }
    }

    pub(crate) fn notify_key_access_changed(&self) {
        (self.ports.refresh_key_consumers)(&self.window);
        if let Some(ports) = self.workflow.borrow().as_ref() {
            (ports.on_key_access_changed)();
        }
    }

    pub(crate) fn mark_generation_page_opened(&self) {
        self.reopen_recipient_page.set(true);
    }

    pub(crate) fn pop_generation_page_if_visible(&self, page: &adw::NavigationPage) {
        if !visible_navigation_page_is(&self.navigation, page) {
            return;
        }

        self.navigation.pop();
        self.notify_generation_page_closed();
    }

    fn notify_generation_page_closed(&self) {
        let reopen_recipient_page = self.reopen_recipient_page.replace(false);
        if let Some(ports) = self.workflow.borrow().as_ref() {
            (ports.on_generation_page_closed)(reopen_recipient_page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RecipientActionPresentation;

    #[test]
    fn standard_key_actions_show_both_groups_without_optional_hardware() {
        let presentation = RecipientActionPresentation::new(true, false, false);

        assert!(presentation.create_group_visible());
        assert!(presentation.add_group_visible(false));
        assert!(presentation.generate_private_key);
        assert!(presentation.import_private_key);
    }

    #[test]
    fn recipient_groups_are_derived_from_policy_not_current_gtk_visibility() {
        let presentation = RecipientActionPresentation::new(true, true, true);

        assert!(presentation.create_group_visible());
        assert!(presentation.add_group_visible(true));
        assert!(presentation.hardware_rows_visible());
    }

    #[test]
    fn blocked_standard_actions_hide_all_action_groups() {
        let presentation = RecipientActionPresentation::new(false, true, true);

        assert!(!presentation.create_group_visible());
        assert!(!presentation.add_group_visible(false));
        assert!(!presentation.hardware_rows_visible());
    }
}
