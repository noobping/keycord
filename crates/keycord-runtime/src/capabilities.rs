//! Generic runtime and host-command capabilities.

use crate::diagnostics::log_info;
use std::ffi::OsString;
#[cfg(feature = "flatpak")]
use std::process::Command;
use std::sync::Once;
#[cfg(feature = "flatpak")]
use std::sync::OnceLock;

pub const HOST_COMMAND_FEATURES_UNSUPPORTED: &str =
    "Host command features are only available on Linux.";
pub const UNSUPPORTED_HOST_COMMAND_ARG: &str = "--unsupported-host-command";

/// Logs the effective feature and permission profile once per process.
pub fn log_runtime_capabilities_once() {
    static RUNTIME_LOGGED: Once = Once::new();

    RUNTIME_LOGGED.call_once(|| {
        log_info(format!(
            "App runtime: debug={}, flatpak={}, logging={}, host-access={}.",
            feature_status(cfg!(debug_assertions)),
            feature_status(cfg!(feature = "flatpak")),
            feature_status(supports_logging_features()),
            feature_status(has_host_permission()),
        ));
    });
}

const fn feature_status(enabled: bool) -> &'static str {
    if enabled {
        "enabled"
    } else {
        "disabled"
    }
}

pub const fn supports_host_command_features() -> bool {
    cfg!(target_os = "linux")
}

pub const fn supports_logging_features() -> bool {
    cfg!(feature = "logging")
}

pub fn require_host_command_features() -> Result<(), String> {
    if supports_host_command_features() {
        Ok(())
    } else {
        Err(HOST_COMMAND_FEATURES_UNSUPPORTED.to_string())
    }
}

pub fn handle_unsupported_host_command_invocation(args: &[OsString]) -> bool {
    if args
        .get(1)
        .is_none_or(|argument| argument != UNSUPPORTED_HOST_COMMAND_ARG)
    {
        return false;
    }

    eprintln!("{HOST_COMMAND_FEATURES_UNSUPPORTED}");
    true
}

#[cfg(feature = "flatpak")]
pub fn has_host_permission() -> bool {
    static HOST_PERMISSION: OnceLock<bool> = OnceLock::new();

    *HOST_PERMISSION.get_or_init(detect_host_permission)
}

#[cfg(not(feature = "flatpak"))]
pub fn has_host_permission() -> bool {
    supports_host_command_features()
}

#[cfg(feature = "flatpak")]
fn detect_host_permission() -> bool {
    detect_host_permission_with(flatpak_host_spawn_probe)
}

#[cfg(feature = "flatpak")]
fn detect_host_permission_with(probe: impl FnOnce() -> bool) -> bool {
    probe()
}

#[cfg(feature = "flatpak")]
fn flatpak_host_spawn_probe() -> bool {
    Command::new("flatpak-spawn")
        .args(["--host", "true"])
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::{
        handle_unsupported_host_command_invocation, OsString, UNSUPPORTED_HOST_COMMAND_ARG,
    };

    #[test]
    fn unsupported_host_command_flag_is_detected() {
        assert!(handle_unsupported_host_command_invocation(&[
            OsString::from("keycord"),
            OsString::from(UNSUPPORTED_HOST_COMMAND_ARG),
        ]));
    }

    #[test]
    fn regular_arguments_do_not_trigger_the_unsupported_host_command_handler() {
        assert!(!handle_unsupported_host_command_invocation(&[
            OsString::from("keycord"),
            OsString::from("--query"),
        ]));
    }

    #[cfg(feature = "flatpak")]
    use super::detect_host_permission_with;

    #[cfg(feature = "flatpak")]
    #[test]
    fn permission_probes_report_their_result() {
        assert!(detect_host_permission_with(|| true));
        assert!(!detect_host_permission_with(|| false));
    }
}
