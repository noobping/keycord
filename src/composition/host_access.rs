//! Connects optional host, smartcard, and FIDO permission UI to application settings.

use adw::prelude::*;

#[cfg(feature = "flatpak")]
use adw::{ActionRow, PreferencesGroup, ToastOverlay};
#[cfg(all(feature = "flatpak", feature = "fidokey"))]
use keycord_fido::has_usb_permission;
#[cfg(feature = "flatpak")]
use keycord_keys::has_smartcard_permission;
#[cfg(feature = "flatpak")]
use keycord_preferences::Preferences;
#[cfg(feature = "flatpak")]
use keycord_runtime::capabilities::has_host_permission;
#[cfg(feature = "flatpak")]
use keycord_runtime::{run_command_output, CommandLogOptions};
#[cfg(feature = "flatpak")]
use keycord_shell::clipboard::set_clipboard_text;
#[cfg(feature = "flatpak")]
use keycord_shell::optional_permission::{
    ensure_optional_permission_row, find_named_action_row, OptionalPermissionRowPorts,
    OptionalPermissionRowSpec,
};
#[cfg(feature = "flatpak")]
use std::rc::Rc;
#[cfg(feature = "flatpak")]
use std::sync::Arc;

#[cfg(feature = "flatpak")]
const APP_ID: &str = env!("APP_ID");
#[cfg(feature = "flatpak")]
const OPTIONAL_HOST_ACCESS_ROW_NAME: &str = "keycord-optional-host-access-row";
#[cfg(feature = "flatpak")]
const OPTIONAL_HOST_ACCESS_NOTICE_ID: &str = "optional-host-access";
#[cfg(feature = "flatpak")]
const FLATPAK_HOST_OVERRIDE_COMMAND: &str = concat!(
    "flatpak override --user --talk-name=org.freedesktop.Flatpak ",
    env!("APP_ID")
);
#[cfg(feature = "flatpak")]
const FLATPAK_HOST_OVERRIDE_ARGS: &[&str] = &[
    "override",
    "--user",
    "--talk-name=org.freedesktop.Flatpak",
    APP_ID,
];
#[cfg(feature = "flatpak")]
const HOST_PERMISSION_CONTEXT: &str = "Grant host access";

#[cfg(feature = "flatpak")]
pub fn append_optional_host_access_group_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
) -> Option<ActionRow> {
    let show_permission_row = !has_host_permission()
        && !Preferences::new().is_notice_hidden(OPTIONAL_HOST_ACCESS_NOTICE_ID);

    let row = find_named_action_row(group, OPTIONAL_HOST_ACCESS_ROW_NAME).or_else(|| {
        let group_for_hide = group.clone();
        let spec = OptionalPermissionRowSpec {
            row_name: OPTIONAL_HOST_ACCESS_ROW_NAME,
            notice_id: OPTIONAL_HOST_ACCESS_NOTICE_ID,
            title: "Allow access to apps on this device",
            subtitle: "Keycord is running in a protected space, so some optional features stay off until you allow this. If you allow it, Keycord can use tools from your computer such as GPG, the Host backend, and pass import. If you don't, Keycord still works with the integrated backend.",
            copy_command: FLATPAK_HOST_OVERRIDE_COMMAND.to_string(),
            command_context: HOST_PERMISSION_CONTEXT,
        };
        let ports = OptionalPermissionRowPorts {
            host_command_access: has_host_permission(),
            persist_hidden_notice: Rc::new(persist_hidden_notice),
            run_permission_command: Arc::new(|| {
                run_flatpak_permission_command(FLATPAK_HOST_OVERRIDE_ARGS, HOST_PERMISSION_CONTEXT)
            }),
            copy_text: Rc::new(copy_permission_command),
            on_hide: Rc::new(move || group_for_hide.set_visible(false)),
        };
        Some(ensure_optional_permission_row(group, overlay, &spec, &ports))
    });

    if let Some(row) = row {
        row.set_visible(show_permission_row);
        group.set_visible(show_permission_row);
        return Some(row);
    }
    group.set_visible(false);
    None
}

#[cfg(not(feature = "flatpak"))]
pub fn append_optional_host_access_group_row(
    group: &adw::PreferencesGroup,
    _overlay: &adw::ToastOverlay,
) -> Option<adw::ActionRow> {
    group.set_visible(false);
    None
}

#[cfg(feature = "flatpak")]
pub fn append_optional_smartcard_access_group_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
    hardware_rows: &[&ActionRow],
    enabled: bool,
) {
    let access = keycord_keys::ui::SmartcardAccessPorts {
        app_id: APP_ID.to_string(),
        smartcard_access_granted: has_smartcard_permission(),
        host_command_access: has_host_permission(),
        notice_hidden: Preferences::new()
            .is_notice_hidden(keycord_keys::ui::SMARTCARD_ACCESS_NOTICE_ID),
        persist_hidden_notice: Rc::new(persist_hidden_notice),
        run_permission_command: Arc::new(|| {
            let args = keycord_keys::ui::flatpak_smartcard_override_args(APP_ID);
            run_flatpak_permission_command_owned(
                &args,
                keycord_keys::ui::SMARTCARD_PERMISSION_CONTEXT,
            )
        }),
        copy_text: Rc::new(copy_permission_command),
    };
    keycord_keys::ui::sync_hardware_key_access_with_flatpak(
        group,
        overlay,
        hardware_rows,
        enabled,
        &access,
    );
}

#[cfg(not(feature = "flatpak"))]
pub fn append_optional_smartcard_access_group_row(
    _group: &adw::PreferencesGroup,
    _overlay: &adw::ToastOverlay,
    hardware_rows: &[&adw::ActionRow],
    enabled: bool,
) {
    keycord_keys::ui::sync_hardware_key_access(hardware_rows, enabled);
}

#[cfg(all(feature = "flatpak", feature = "fidokey"))]
pub fn append_optional_fido2_access_group_row(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
    fido2_rows: &[&ActionRow],
    enabled: bool,
) {
    let preferences = Preferences::new();
    let ports = keycord_fido::ui::UsbAccessPorts {
        app_id: APP_ID.to_string(),
        usb_access_granted: has_usb_permission(),
        host_command_access: has_host_permission(),
        notice_hidden: preferences.is_notice_hidden(keycord_fido::ui::USB_ACCESS_NOTICE_ID),
        persist_hidden_notice: Rc::new(persist_hidden_notice),
        run_permission_command: Arc::new(run_fido2_permission_command),
        copy_text: Rc::new(copy_permission_command),
    };
    keycord_fido::ui::sync_generation_access(group, overlay, fido2_rows, enabled, Some(&ports));
}

#[cfg(all(not(feature = "flatpak"), feature = "fidokey"))]
pub fn append_optional_fido2_access_group_row(
    group: &adw::PreferencesGroup,
    overlay: &adw::ToastOverlay,
    fido2_rows: &[&adw::ActionRow],
    enabled: bool,
) {
    keycord_fido::ui::sync_generation_access(group, overlay, fido2_rows, enabled, None);
}

#[cfg(feature = "flatpak")]
fn persist_hidden_notice(notice_id: &str) -> Result<(), String> {
    Preferences::new()
        .hide_notice(notice_id)
        .map_err(|error| error.to_string())
}

#[cfg(feature = "flatpak")]
fn copy_permission_command(text: &str, overlay: &ToastOverlay, button: &adw::gtk::Button) -> bool {
    set_clipboard_text(text, overlay, Some(button))
}

#[cfg(feature = "flatpak")]
fn run_flatpak_permission_command(args: &[&str], context: &str) -> Result<(), String> {
    let mut command = Preferences::new().host_program_command("flatpak", args);
    let output = run_command_output(&mut command, context, CommandLogOptions::DEFAULT)
        .map_err(|error| format!("Failed to start the permission command: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(keycord_runtime::output_failure_message(
            &output,
            "Permission command failed",
        ))
    }
}

#[cfg(feature = "flatpak")]
fn run_flatpak_permission_command_owned(args: &[String], context: &str) -> Result<(), String> {
    let args = args.iter().map(String::as_str).collect::<Vec<_>>();
    run_flatpak_permission_command(&args, context)
}

#[cfg(all(feature = "flatpak", feature = "fidokey"))]
fn run_fido2_permission_command() -> Result<(), String> {
    let args = keycord_fido::ui::flatpak_usb_override_args(APP_ID);
    run_flatpak_permission_command_owned(&args, keycord_fido::ui::USB_PERMISSION_CONTEXT)
}
