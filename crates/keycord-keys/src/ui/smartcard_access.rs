//! Smartcard-specific optional sandbox access presentation.

use adw::prelude::*;
use adw::ActionRow;
use keycord_runtime::i18n::gettext;

const SMARTCARD_ACCESS_REQUIRED_TOOLTIP: &str = "Grant smartcard access first.";

#[cfg(feature = "flatpak")]
use adw::{PreferencesGroup, ToastOverlay};
#[cfg(feature = "flatpak")]
use keycord_shell::optional_permission::{
    ensure_optional_permission_row, find_named_action_row, CopyText, OptionalPermissionRowPorts,
    OptionalPermissionRowSpec, PersistHiddenNotice, RunPermissionCommand,
};
#[cfg(feature = "flatpak")]
use std::rc::Rc;

#[cfg(feature = "flatpak")]
pub const SMARTCARD_ACCESS_NOTICE_ID: &str = "optional-smartcard-access";
#[cfg(feature = "flatpak")]
pub const SMARTCARD_ACCESS_ROW_NAME: &str = "keycord-optional-smartcard-access-row";
#[cfg(feature = "flatpak")]
pub const SMARTCARD_PERMISSION_CONTEXT: &str = "Grant smartcard access";

#[cfg(feature = "flatpak")]
const SMARTCARD_ACCESS_TITLE: &str = "Allow smartcard access (Experimental)";
#[cfg(feature = "flatpak")]
const SMARTCARD_ACCESS_SUBTITLE: &str =
    "Experimental hardware-key workflows need PC/SC access to use connected OpenPGP smartcards or YubiKeys, then restart Keycord. Password-protected keys remain available without this.";

/// Application-owned ports for smartcard sandbox access.
#[cfg(feature = "flatpak")]
#[derive(Clone)]
pub struct SmartcardAccessPorts {
    pub app_id: String,
    pub smartcard_access_granted: bool,
    pub host_command_access: bool,
    pub notice_hidden: bool,
    pub persist_hidden_notice: PersistHiddenNotice,
    pub run_permission_command: RunPermissionCommand,
    pub copy_text: CopyText,
}

pub fn sync_hardware_key_access(hardware_rows: &[&ActionRow], enabled: bool) {
    let tooltip = (!enabled).then(|| gettext(SMARTCARD_ACCESS_REQUIRED_TOOLTIP));
    for row in hardware_rows {
        row.set_sensitive(enabled);
        row.set_tooltip_text(tooltip.as_deref());
    }
}

#[cfg(feature = "flatpak")]
pub fn sync_hardware_key_access_with_flatpak(
    group: &PreferencesGroup,
    overlay: &ToastOverlay,
    hardware_rows: &[&ActionRow],
    enabled: bool,
    access: &SmartcardAccessPorts,
) {
    let presentation = flatpak_access_presentation(
        enabled,
        access.smartcard_access_granted,
        access.notice_hidden,
    );
    let tooltip = presentation.tooltip.map(gettext);
    for row in hardware_rows {
        row.set_sensitive(presentation.sensitive);
        row.set_tooltip_text(tooltip.as_deref());
    }

    if let Some(row) = find_named_action_row(group, SMARTCARD_ACCESS_ROW_NAME) {
        row.set_visible(presentation.show_permission_row);
    }
    if !presentation.show_permission_row {
        return;
    }

    let spec = OptionalPermissionRowSpec {
        row_name: SMARTCARD_ACCESS_ROW_NAME,
        notice_id: SMARTCARD_ACCESS_NOTICE_ID,
        title: SMARTCARD_ACCESS_TITLE,
        subtitle: SMARTCARD_ACCESS_SUBTITLE,
        copy_command: flatpak_smartcard_override_command(&access.app_id),
        command_context: SMARTCARD_PERMISSION_CONTEXT,
    };
    let ports = OptionalPermissionRowPorts {
        host_command_access: access.host_command_access,
        persist_hidden_notice: access.persist_hidden_notice.clone(),
        run_permission_command: access.run_permission_command.clone(),
        copy_text: access.copy_text.clone(),
        on_hide: Rc::new(|| {}),
    };
    ensure_optional_permission_row(group, overlay, &spec, &ports).set_visible(true);
}

#[cfg(feature = "flatpak")]
pub fn flatpak_smartcard_override_command(app_id: &str) -> String {
    format!("flatpak override --user --socket=pcsc {app_id}")
}

#[cfg(feature = "flatpak")]
pub fn flatpak_smartcard_override_args(app_id: &str) -> Vec<String> {
    vec![
        "override".to_string(),
        "--user".to_string(),
        "--socket=pcsc".to_string(),
        app_id.to_string(),
    ]
}

#[cfg(feature = "flatpak")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SmartcardAccessPresentation {
    sensitive: bool,
    show_permission_row: bool,
    tooltip: Option<&'static str>,
}

#[cfg(feature = "flatpak")]
const fn flatpak_access_presentation(
    enabled: bool,
    smartcard_access_granted: bool,
    notice_hidden: bool,
) -> SmartcardAccessPresentation {
    SmartcardAccessPresentation {
        sensitive: enabled && smartcard_access_granted,
        show_permission_row: enabled && !smartcard_access_granted && !notice_hidden,
        tooltip: if enabled && !smartcard_access_granted {
            Some(SMARTCARD_ACCESS_REQUIRED_TOOLTIP)
        } else {
            None
        },
    }
}

#[cfg(all(test, feature = "flatpak"))]
mod tests {
    use super::{
        flatpak_access_presentation, flatpak_smartcard_override_args,
        flatpak_smartcard_override_command, SmartcardAccessPresentation,
        SMARTCARD_ACCESS_REQUIRED_TOOLTIP,
    };

    #[test]
    fn flatpak_smartcard_command_preserves_app_id_and_pcsc_socket() {
        assert_eq!(
            flatpak_smartcard_override_command("io.example.App"),
            "flatpak override --user --socket=pcsc io.example.App"
        );
        assert_eq!(
            flatpak_smartcard_override_args("io.example.App"),
            ["override", "--user", "--socket=pcsc", "io.example.App"]
        );
    }

    #[test]
    fn flatpak_smartcard_access_state_controls_rows_and_notice() {
        assert_eq!(
            flatpak_access_presentation(true, false, false),
            SmartcardAccessPresentation {
                sensitive: false,
                show_permission_row: true,
                tooltip: Some(SMARTCARD_ACCESS_REQUIRED_TOOLTIP),
            }
        );
        assert_eq!(
            flatpak_access_presentation(true, true, false),
            SmartcardAccessPresentation {
                sensitive: true,
                show_permission_row: false,
                tooltip: None,
            }
        );
        assert!(!flatpak_access_presentation(true, false, true).show_permission_row);
    }
}
