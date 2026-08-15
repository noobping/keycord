//! Generic presentation for optional sandbox permissions.

use crate::background::spawn_result_task;
use crate::qr_code::{connect_qr_button, copy_qr_button_group};
use crate::ui::{add_persistent_hide_button_with, flat_icon_button_with_tooltip};
use adw::gtk::{Align, Box as GtkBox, Button, Orientation};
use adw::prelude::*;
use adw::{ActionRow, PreferencesGroup, Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use std::rc::Rc;
use std::sync::Arc;

const PERMISSION_SUCCESS_TOAST: &str = "Restart app to apply.";
const PERMISSION_ERROR_TOAST: &str = "Couldn't grant permission.";

pub type PersistHiddenNotice = Rc<dyn Fn(&str) -> Result<(), String>>;
pub type RunPermissionCommand = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
pub type CopyText = Rc<dyn Fn(&str, &ToastOverlay, &Button) -> bool>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalPermissionRowSpec {
    pub row_name: &'static str,
    pub notice_id: &'static str,
    pub title: &'static str,
    pub subtitle: &'static str,
    pub copy_command: String,
    pub command_context: &'static str,
}

#[derive(Clone)]
pub struct OptionalPermissionRowPorts {
    pub host_command_access: bool,
    pub persist_hidden_notice: PersistHiddenNotice,
    pub run_permission_command: RunPermissionCommand,
    pub copy_text: CopyText,
    pub on_hide: Rc<dyn Fn()>,
}

pub fn ensure_optional_permission_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
    spec: &OptionalPermissionRowSpec,
    ports: &OptionalPermissionRowPorts,
) -> ActionRow {
    if let Some(row) = find_named_action_row(group, spec.row_name) {
        return row;
    }

    let row = ActionRow::builder()
        .title(gettext(spec.title))
        .subtitle(gettext(spec.subtitle))
        .build();
    row.set_activatable(false);

    if uses_in_app_grant(ports.host_command_access) {
        append_grant_button(&row, overlay, spec, ports);
    } else {
        append_copy_buttons(&row, overlay, spec, ports);
    }

    row.set_widget_name(spec.row_name);
    let persist_hidden_notice = ports.persist_hidden_notice.clone();
    let on_hide = ports.on_hide.clone();
    add_persistent_hide_button_with(
        &row,
        spec.notice_id,
        move |notice_id| persist_hidden_notice(notice_id),
        move || on_hide(),
    );
    group.add(&row);
    row
}

pub const fn uses_in_app_grant(host_command_access: bool) -> bool {
    host_command_access
}

const fn grant_retry_enabled(command_succeeded: bool) -> bool {
    !command_succeeded
}

fn append_grant_button(
    row: &ActionRow,
    overlay: &ToastOverlay,
    spec: &OptionalPermissionRowSpec,
    ports: &OptionalPermissionRowPorts,
) {
    let button = Button::with_label(&gettext("Grant"));
    button.add_css_class("suggested-action");
    button.set_tooltip_text(Some(&gettext("Grant permission")));

    let run_permission_command = ports.run_permission_command.clone();
    let context = spec.command_context;
    let overlay = overlay.clone();
    button.connect_clicked(move |button| {
        button.set_sensitive(false);
        let run_permission_command = run_permission_command.clone();
        let overlay_for_result = overlay.clone();
        let overlay_for_disconnect = overlay.clone();
        let button_for_result = button.clone();
        let button_for_disconnect = button.clone();
        spawn_result_task(
            move || run_permission_command(),
            move |result| {
                button_for_result.set_sensitive(grant_retry_enabled(result.is_ok()));
                match result {
                    Ok(()) => {
                        overlay_for_result.add_toast(Toast::new(&gettext(PERMISSION_SUCCESS_TOAST)))
                    }
                    Err(error) => {
                        log_error(format!("{context}: {error}"));
                        overlay_for_result.add_toast(Toast::new(&gettext(PERMISSION_ERROR_TOAST)));
                    }
                }
            },
            move || {
                button_for_disconnect.set_sensitive(true);
                overlay_for_disconnect.add_toast(Toast::new(&gettext(PERMISSION_ERROR_TOAST)));
            },
        );
    });

    let suffix = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .valign(Align::Center)
        .build();
    suffix.append(&button);
    row.add_suffix(&suffix);
}

fn append_copy_buttons(
    row: &ActionRow,
    overlay: &ToastOverlay,
    spec: &OptionalPermissionRowSpec,
    ports: &OptionalPermissionRowPorts,
) {
    let copy_button =
        flat_icon_button_with_tooltip("edit-copy-symbolic", "Copy permission command");
    let (button_group, qr_button) =
        copy_qr_button_group(&copy_button, "Show permission command as QR code");
    row.add_suffix(&button_group);

    let command = spec.copy_command.clone();
    let command_for_qr = command.clone();
    connect_qr_button(&qr_button, overlay, move || command_for_qr.clone());

    let copy_text = ports.copy_text.clone();
    let overlay = overlay.clone();
    let feedback_button = copy_button.clone();
    copy_button.connect_clicked(move |_| {
        if copy_text(&command, &overlay, &feedback_button) {
            overlay.add_toast(Toast::new(&gettext("Copied.")));
        }
    });
}

pub fn find_named_action_row(group: &PreferencesGroup, widget_name: &str) -> Option<ActionRow> {
    find_named_descendant_action_row(group.upcast_ref(), widget_name)
}

fn find_named_descendant_action_row(
    widget: &adw::gtk::Widget,
    widget_name: &str,
) -> Option<ActionRow> {
    if widget.widget_name() == widget_name {
        return widget.clone().downcast::<ActionRow>().ok();
    }

    let mut child = widget.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if let Some(row) = find_named_descendant_action_row(&widget, widget_name) {
            return Some(row);
        }
        child = next;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{grant_retry_enabled, uses_in_app_grant};

    #[test]
    fn host_command_access_selects_in_app_grant() {
        assert!(!uses_in_app_grant(false));
        assert!(uses_in_app_grant(true));
    }

    #[test]
    fn successful_grants_stay_disabled_while_failures_allow_retry() {
        assert!(!grant_retry_enabled(true));
        assert!(grant_retry_enabled(false));
    }
}
