//! GTK presentation owned by the FIDO subject.

use adw::gtk::{Align, Box as GtkBox, Builder, Label, Orientation, Widget};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, Dialog, PasswordEntryRow, PreferencesGroup, PreferencesPage,
    ToastOverlay,
};
use keycord_runtime::diagnostics::log_error;
use keycord_runtime::i18n::gettext;
use keycord_shell::background::spawn_result_task_with_finalizer;
use keycord_shell::optional_permission::{
    ensure_optional_permission_row, find_named_action_row, CopyText, OptionalPermissionRowPorts,
    OptionalPermissionRowSpec, PersistHiddenNotice, RunPermissionCommand,
};
use keycord_shell::ui::{
    build_progress_dialog, connect_password_entry_row_apply_button_to_nonempty_text,
    connect_row_action, dialog_content_shell, required_builder_object,
};
use secrecy::{ExposeSecret, SecretString};
use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use crate::FidoErrorKind;

pub const USB_ACCESS_NOTICE_ID: &str = "optional-fido2-access";
pub const USB_ACCESS_ROW_NAME: &str = "keycord-optional-fido2-access-row";
pub const USB_PERMISSION_CONTEXT: &str = "Grant USB security key access (Experimental FIDO2)";

const BACKEND_REQUIRED_TOOLTIP: &str =
    "Switch to the Integrated backend to use experimental FIDO2-protected private keys.";
const PERMISSION_REQUIRED_TOOLTIP: &str =
    "Grant USB security key access for experimental FIDO2-protected private keys first.";
const USB_ACCESS_TITLE: &str = "Allow USB security key access (Experimental FIDO2)";
const USB_ACCESS_SUBTITLE: &str =
    "Experimental FIDO2-protected private keys need USB device access before Keycord can use a connected security key to unlock protected private keys. Restart Keycord after granting access.";
const TOUCH_DESCRIPTION: &str = "Touch your key if it blinks.";

/// FIDO-owned widgets embedded in the Keys recipient workflow.
#[derive(Clone)]
pub struct FidoWindowWidgets {
    generation_row: ActionRow,
}

impl FidoWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        Ok(Self {
            generation_row: required_builder_object(
                builder,
                "store_recipients_generate_fido2_key_row",
            )?,
        })
    }

    pub fn generation_row(&self) -> &ActionRow {
        &self.generation_row
    }

    pub fn sync_generation_visibility(&self, actions_enabled: bool) {
        self.generation_row
            .set_visible(actions_enabled && crate::security_key_available());
    }

    pub fn generation_is_visible(&self) -> bool {
        self.generation_row.is_visible()
    }

    /// Search widgets contributed to the embedding key-management group.
    pub fn recipient_search_widgets(&self) -> Vec<Widget> {
        vec![self.generation_row.clone().upcast()]
    }

    pub fn connect_generation_workflow(
        &self,
        window: &ApplicationWindow,
        overlay: &ToastOverlay,
        ports: FidoKeyGenerationUiPorts,
    ) {
        let state = FidoKeyGenerationUiState {
            window: window.clone(),
            overlay: overlay.clone(),
            ports,
        };
        connect_row_action(&self.generation_row, move || {
            start_key_generation(&state, None);
        });
    }
}

/// Presentation-ready failure returned by the OpenPGP adapter.
///
/// FIDO owns retry policy and presentation. The caller retains ownership of
/// its domain error and translates only the details needed by this workflow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidoKeyGenerationError {
    kind: Option<FidoErrorKind>,
    detail: String,
    user_message: String,
}

impl FidoKeyGenerationError {
    pub fn new(
        kind: Option<FidoErrorKind>,
        detail: impl Into<String>,
        user_message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            detail: detail.into(),
            user_message: user_message.into(),
        }
    }
}

pub type GenerateFidoProtectedKey =
    Arc<dyn Fn(Option<SecretString>) -> Result<(), FidoKeyGenerationError> + Send + Sync>;
pub type SetFidoPinAndGenerate =
    Arc<dyn Fn(SecretString) -> Result<(), FidoKeyGenerationError> + Send + Sync>;

/// OpenPGP adapter ports consumed by the FIDO-owned generation workflow.
#[derive(Clone)]
pub struct FidoKeyGenerationUiPorts {
    pub actions_allowed: Rc<dyn Fn() -> bool>,
    pub generate: GenerateFidoProtectedKey,
    pub set_pin_and_generate: SetFidoPinAndGenerate,
    pub on_generated: Rc<dyn Fn()>,
}

#[derive(Clone)]
struct FidoKeyGenerationUiState {
    window: ApplicationWindow,
    overlay: ToastOverlay,
    ports: FidoKeyGenerationUiPorts,
}

fn finish_key_generation(
    state: &FidoKeyGenerationUiState,
    result: Result<(), FidoKeyGenerationError>,
) {
    match result {
        Ok(()) => {
            (state.ports.on_generated)();
            state
                .overlay
                .add_toast(adw::Toast::new(&gettext("Key generated.")));
        }
        Err(error) => {
            log_error(format!(
                "Failed to generate experimental FIDO2-protected key: {}",
                error.detail
            ));
            state
                .overlay
                .add_toast(adw::Toast::new(&gettext(&error.user_message)));
        }
    }
}

fn start_key_generation(state: &FidoKeyGenerationUiState, pin: Option<SecretString>) {
    if !(state.ports.actions_allowed)() {
        return;
    }

    let progress_dialog = present_progress_dialog(
        &state.window,
        "Generating FIDO2-protected key (Experimental)",
        None,
    );
    let generate = state.ports.generate.clone();
    let pin_was_supplied = pin.is_some();
    let state_for_result = state.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || generate(pin),
        move || progress_dialog.force_close(),
        move |result| match result {
            Err(error)
                if error.kind == Some(FidoErrorKind::PinNotSet)
                    && !pin_was_supplied
                    && crate::FidoService::supports_pin_setup() =>
            {
                prompt_key_pin_setup(&state_for_result);
            }
            Err(error) if error.kind == Some(FidoErrorKind::PinRequired) && !pin_was_supplied => {
                prompt_key_pin(&state_for_result);
            }
            other => finish_key_generation(&state_for_result, other),
        },
        move || {
            log_error(
                "Experimental FIDO2-protected key generation worker disconnected unexpectedly."
                    .to_string(),
            );
            state_for_disconnect
                .overlay
                .add_toast(adw::Toast::new(&gettext("Couldn't generate the key.")));
        },
    );
}

fn start_pin_setup_and_generation(state: &FidoKeyGenerationUiState, pin: SecretString) {
    if !(state.ports.actions_allowed)() {
        return;
    }

    let progress_dialog = present_progress_dialog(
        &state.window,
        "Set security key PIN (Experimental FIDO2)",
        None,
    );
    let set_pin_and_generate = state.ports.set_pin_and_generate.clone();
    let state_for_result = state.clone();
    let state_for_disconnect = state.clone();
    spawn_result_task_with_finalizer(
        move || set_pin_and_generate(pin),
        move || progress_dialog.force_close(),
        move |result| finish_key_generation(&state_for_result, result),
        move || {
            log_error("FIDO2 PIN setup worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(adw::Toast::new(&gettext("Couldn't generate the key.")));
        },
    );
}

fn prompt_key_pin(state: &FidoKeyGenerationUiState) {
    let window = state.window.clone();
    let state = state.clone();
    present_pin_entry_dialog(
        &window,
        "Generate FIDO2-protected key (Experimental)",
        None,
        move |pin| start_key_generation(&state, Some(pin)),
        || {},
    );
}

fn prompt_key_pin_setup(state: &FidoKeyGenerationUiState) {
    let window = state.window.clone();
    let state = state.clone();
    present_pin_setup_dialog(
        &window,
        "Set security key PIN (Experimental FIDO2)",
        None,
        move |pin| start_pin_setup_and_generation(&state, pin),
        || {},
    );
}

/// Application-owned ports needed by the Flatpak USB permission presentation.
///
/// The FIDO crate owns the subject copy and widget behavior while preferences,
/// clipboard access, and process policy stay in their respective subjects.
#[derive(Clone)]
pub struct UsbAccessPorts {
    pub app_id: String,
    pub usb_access_granted: bool,
    pub host_command_access: bool,
    pub notice_hidden: bool,
    pub persist_hidden_notice: PersistHiddenNotice,
    pub run_permission_command: RunPermissionCommand,
    pub copy_text: CopyText,
}

pub fn flatpak_usb_override_command(app_id: &str) -> String {
    format!("flatpak override --user --device=all {app_id}")
}

pub fn flatpak_usb_override_args(app_id: &str) -> Vec<String> {
    vec![
        "override".to_string(),
        "--user".to_string(),
        "--device=all".to_string(),
        app_id.to_string(),
    ]
}

pub fn present_progress_dialog(
    window: &ApplicationWindow,
    title: &str,
    subtitle: Option<&str>,
) -> Dialog {
    build_progress_dialog(window, title, subtitle, TOUCH_DESCRIPTION)
}

fn pin_entry_error_message(pin: &str) -> Option<&'static str> {
    pin.trim()
        .is_empty()
        .then_some("Enter the security key PIN.")
}

fn pin_setup_error_message(pin: &str, confirmation: &str) -> Option<&'static str> {
    if pin.trim().is_empty() {
        return Some("Enter the new security key PIN.");
    }
    if confirmation.trim().is_empty() {
        return Some("Confirm the new security key PIN.");
    }
    if pin != confirmation {
        return Some("The security key PINs do not match.");
    }
    None
}

fn error_label() -> Label {
    let label = Label::new(None);
    label.set_halign(Align::Start);
    label.set_wrap(true);
    label.add_css_class("error");
    label.add_css_class("caption");
    label.set_margin_top(6);
    label.set_margin_start(18);
    label.set_margin_end(18);
    label.set_margin_bottom(18);
    label.set_visible(false);
    label
}

pub fn present_pin_entry_dialog<F, G>(
    window: &ApplicationWindow,
    title: &str,
    subtitle: Option<&str>,
    on_submit: F,
    on_close: G,
) where
    F: Fn(SecretString) + 'static,
    G: Fn() + 'static,
{
    let pin_row = PasswordEntryRow::new();
    pin_row.set_title(&gettext("Security key PIN"));
    pin_row.set_show_apply_button(true);
    connect_password_entry_row_apply_button_to_nonempty_text(&pin_row);

    let pin_group = PreferencesGroup::new();
    pin_group.add(&pin_row);
    let page = PreferencesPage::new();
    page.add(&pin_group);

    let error_label = error_label();
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    content.append(&error_label);

    let dialog = Dialog::builder()
        .title(gettext(title))
        .content_height(280)
        .content_width(800)
        .follows_content_size(true)
        .child(&dialog_content_shell(title, subtitle, &content))
        .build();
    let submitted = Rc::new(Cell::new(false));

    let submitted_for_apply = submitted.clone();
    let dialog_for_apply = dialog.clone();
    let error_label_for_apply = error_label.clone();
    pin_row.connect_apply(move |row| {
        let pin = SecretString::from(row.text().as_str());
        if let Some(message) = pin_entry_error_message(pin.expose_secret()) {
            error_label_for_apply.set_label(&gettext(message));
            error_label_for_apply.set_visible(true);
            return;
        }

        error_label_for_apply.set_visible(false);
        submitted_for_apply.set(true);
        row.set_text("");
        dialog_for_apply.force_close();
        on_submit(pin);
    });

    {
        let error_label = error_label.clone();
        pin_row.connect_changed(move |_| error_label.set_visible(false));
    }

    let pin_row_for_close = pin_row.clone();
    dialog.connect_closed(move |_| {
        pin_row_for_close.set_text("");
        if !submitted.get() {
            on_close();
        }
    });
    dialog.present(Some(window));
}

fn sync_pin_setup_apply_button(pin_row: &PasswordEntryRow, confirm_row: &PasswordEntryRow) {
    confirm_row.set_show_apply_button(
        !pin_row.text().trim().is_empty() && !confirm_row.text().trim().is_empty(),
    );
}

pub fn present_pin_setup_dialog<F, G>(
    window: &ApplicationWindow,
    title: &str,
    subtitle: Option<&str>,
    on_submit: F,
    on_close: G,
) where
    F: Fn(SecretString) + 'static,
    G: Fn() + 'static,
{
    let pin_row = PasswordEntryRow::new();
    pin_row.set_title(&gettext("New security key PIN"));
    pin_row.set_show_apply_button(false);

    let confirm_row = PasswordEntryRow::new();
    confirm_row.set_title(&gettext("Confirm new security key PIN"));
    confirm_row.set_show_apply_button(false);

    let pin_group = PreferencesGroup::new();
    pin_group.add(&pin_row);
    pin_group.add(&confirm_row);
    let page = PreferencesPage::new();
    page.add(&pin_group);

    let error_label = error_label();
    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    content.append(&error_label);

    let dialog = Dialog::builder()
        .title(gettext(title))
        .content_height(320)
        .content_width(800)
        .follows_content_size(true)
        .child(&dialog_content_shell(title, subtitle, &content))
        .build();
    let submitted = Rc::new(Cell::new(false));

    {
        let confirm_row = confirm_row.clone();
        pin_row.connect_changed(move |row| sync_pin_setup_apply_button(row, &confirm_row));
    }
    {
        let pin_row = pin_row.clone();
        confirm_row.connect_changed(move |row| sync_pin_setup_apply_button(&pin_row, row));
    }
    {
        let error_label = error_label.clone();
        pin_row.connect_changed(move |_| error_label.set_visible(false));
    }
    {
        let error_label = error_label.clone();
        confirm_row.connect_changed(move |_| error_label.set_visible(false));
    }

    let submitted_for_apply = submitted.clone();
    let dialog_for_apply = dialog.clone();
    let error_label_for_apply = error_label.clone();
    let pin_row_for_apply = pin_row.clone();
    confirm_row.connect_apply(move |row| {
        let pin = SecretString::from(pin_row_for_apply.text().as_str());
        let confirmation = row.text();
        if let Some(message) = pin_setup_error_message(pin.expose_secret(), confirmation.as_str()) {
            error_label_for_apply.set_label(&gettext(message));
            error_label_for_apply.set_visible(true);
            return;
        }

        error_label_for_apply.set_visible(false);
        submitted_for_apply.set(true);
        pin_row_for_apply.set_text("");
        row.set_text("");
        dialog_for_apply.force_close();
        on_submit(pin);
    });

    let pin_row_for_close = pin_row.clone();
    let confirm_row_for_close = confirm_row.clone();
    dialog.connect_closed(move |_| {
        pin_row_for_close.set_text("");
        confirm_row_for_close.set_text("");
        if !submitted.get() {
            on_close();
        }
    });
    dialog.present(Some(window));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinRetryPrompt {
    Entry,
    Setup,
}

fn pin_retry_prompt(kind: FidoErrorKind, allow_retry: bool) -> Option<PinRetryPrompt> {
    if !allow_retry {
        return None;
    }

    match kind {
        FidoErrorKind::PinNotSet if crate::FidoService::supports_pin_setup() => {
            Some(PinRetryPrompt::Setup)
        }
        FidoErrorKind::PinRequired | FidoErrorKind::TokenNotPresent => Some(PinRetryPrompt::Entry),
        _ => None,
    }
}

/// Present the FIDO-owned PIN retry UI selected for a failed key operation.
///
/// The caller supplies only the OpenPGP continuation callbacks. Retry policy,
/// titles, PIN validation, and dialog lifecycle remain in this subject.
pub fn present_private_key_pin_retry_dialog<F, G, H>(
    window: &ApplicationWindow,
    subtitle: Option<&str>,
    kind: FidoErrorKind,
    allow_retry: bool,
    on_pin_entry: F,
    on_pin_setup: G,
    on_close: H,
) -> bool
where
    F: Fn(SecretString) + 'static,
    G: Fn(SecretString) + 'static,
    H: Fn() + 'static,
{
    match pin_retry_prompt(kind, allow_retry) {
        Some(PinRetryPrompt::Entry) => {
            present_pin_entry_dialog(window, "Unlock key", subtitle, on_pin_entry, on_close);
            true
        }
        Some(PinRetryPrompt::Setup) => {
            present_pin_setup_dialog(
                window,
                "Set security key PIN",
                subtitle,
                on_pin_setup,
                on_close,
            );
            true
        }
        None => false,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AccessPresentation {
    sensitive: bool,
    show_permission_row: bool,
    tooltip: Option<&'static str>,
}

fn access_presentation(enabled: bool, access: Option<&UsbAccessPorts>) -> AccessPresentation {
    let Some(access) = access else {
        return AccessPresentation {
            sensitive: enabled,
            show_permission_row: false,
            tooltip: (!enabled).then_some(BACKEND_REQUIRED_TOOLTIP),
        };
    };

    AccessPresentation {
        sensitive: enabled && access.usb_access_granted,
        show_permission_row: enabled && !access.usb_access_granted && !access.notice_hidden,
        tooltip: if !enabled {
            Some(BACKEND_REQUIRED_TOOLTIP)
        } else if !access.usb_access_granted {
            Some(PERMISSION_REQUIRED_TOOLTIP)
        } else {
            None
        },
    }
}

pub fn sync_generation_access(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
    generation_rows: &[&ActionRow],
    enabled: bool,
    access: Option<&UsbAccessPorts>,
) {
    let presentation = access_presentation(enabled, access);
    let tooltip = presentation.tooltip.map(gettext);
    for row in generation_rows {
        row.set_sensitive(presentation.sensitive);
        row.set_tooltip_text(tooltip.as_deref());
    }

    if let Some(row) = find_named_action_row(group, USB_ACCESS_ROW_NAME) {
        row.set_visible(presentation.show_permission_row);
    }
    if !presentation.show_permission_row {
        return;
    }

    let Some(access) = access else {
        return;
    };
    ensure_usb_access_row(group, overlay, access).set_visible(true);
}

fn ensure_usb_access_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
    access: &UsbAccessPorts,
) -> ActionRow {
    let spec = OptionalPermissionRowSpec {
        row_name: USB_ACCESS_ROW_NAME,
        notice_id: USB_ACCESS_NOTICE_ID,
        title: USB_ACCESS_TITLE,
        subtitle: USB_ACCESS_SUBTITLE,
        copy_command: flatpak_usb_override_command(&access.app_id),
        command_context: USB_PERMISSION_CONTEXT,
    };
    let ports = OptionalPermissionRowPorts {
        host_command_access: access.host_command_access,
        persist_hidden_notice: access.persist_hidden_notice.clone(),
        run_permission_command: access.run_permission_command.clone(),
        copy_text: access.copy_text.clone(),
        on_hide: Rc::new(|| {}),
    };
    ensure_optional_permission_row(group, overlay, &spec, &ports)
}

#[cfg(test)]
mod tests {
    use super::{
        access_presentation, flatpak_usb_override_args, flatpak_usb_override_command,
        pin_entry_error_message, pin_retry_prompt, pin_setup_error_message, AccessPresentation,
        PinRetryPrompt,
    };
    use crate::FidoErrorKind;

    #[test]
    fn pin_entry_requires_a_value() {
        assert_eq!(
            pin_entry_error_message("  "),
            Some("Enter the security key PIN.")
        );
        assert_eq!(pin_entry_error_message("123456"), None);
    }

    #[test]
    fn pin_setup_requires_matching_nonempty_values() {
        assert_eq!(
            pin_setup_error_message("", ""),
            Some("Enter the new security key PIN.")
        );
        assert_eq!(
            pin_setup_error_message("123456", ""),
            Some("Confirm the new security key PIN.")
        );
        assert_eq!(
            pin_setup_error_message("123456", "654321"),
            Some("The security key PINs do not match.")
        );
        assert_eq!(pin_setup_error_message("123456", "123456"), None);
    }

    #[test]
    fn flatpak_override_targets_only_the_requested_app() {
        assert_eq!(
            flatpak_usb_override_command("io.example.Keycord"),
            "flatpak override --user --device=all io.example.Keycord"
        );
        assert_eq!(
            flatpak_usb_override_args("io.example.Keycord"),
            ["override", "--user", "--device=all", "io.example.Keycord"]
        );
    }

    #[test]
    fn non_sandboxed_generation_only_depends_on_backend_availability() {
        assert_eq!(
            access_presentation(true, None),
            AccessPresentation {
                sensitive: true,
                show_permission_row: false,
                tooltip: None,
            }
        );
        assert!(!access_presentation(false, None).sensitive);
    }

    #[test]
    fn pin_retry_policy_is_owned_by_fido() {
        assert_eq!(
            pin_retry_prompt(FidoErrorKind::PinRequired, true),
            Some(PinRetryPrompt::Entry)
        );
        assert_eq!(
            pin_retry_prompt(FidoErrorKind::TokenNotPresent, true),
            Some(PinRetryPrompt::Entry)
        );
        assert_eq!(pin_retry_prompt(FidoErrorKind::IncorrectPin, true), None);
        assert_eq!(pin_retry_prompt(FidoErrorKind::PinRequired, false), None);
    }
}
