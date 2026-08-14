//! Key-management feature availability and smartcard permission probes.

#[cfg(feature = "flatpak")]
use std::env;
#[cfg(feature = "flatpak")]
use std::sync::OnceLock;

pub const fn smartcard_available() -> bool {
    cfg!(feature = "smartcard")
}

pub const fn hardware_key_available() -> bool {
    cfg!(feature = "hardwarekey")
}

pub const fn host_private_key_sync_available() -> bool {
    cfg!(all(target_os = "linux", feature = "flatpak"))
}

#[cfg(feature = "flatpak")]
pub fn has_smartcard_permission() -> bool {
    static SMARTCARD_PERMISSION: OnceLock<bool> = OnceLock::new();

    smartcard_available() && *SMARTCARD_PERMISSION.get_or_init(detect_smartcard_permission)
}

#[cfg(not(feature = "flatpak"))]
pub const fn has_smartcard_permission() -> bool {
    smartcard_available()
}

#[cfg(feature = "flatpak")]
fn detect_smartcard_permission() -> bool {
    detect_smartcard_permission_with(flatpak_pcsc_socket_probe)
}

#[cfg(feature = "flatpak")]
fn detect_smartcard_permission_with(probe: impl FnOnce() -> bool) -> bool {
    probe()
}

#[cfg(feature = "flatpak")]
fn flatpak_pcsc_socket_probe() -> bool {
    env::var_os("PCSCLITE_CSOCK_NAME").is_some()
}

#[cfg(test)]
mod tests {
    use super::{hardware_key_available, host_private_key_sync_available, smartcard_available};

    #[test]
    fn availability_matches_key_features() {
        assert_eq!(smartcard_available(), cfg!(feature = "smartcard"));
        assert_eq!(hardware_key_available(), cfg!(feature = "hardwarekey"));
        assert_eq!(
            host_private_key_sync_available(),
            cfg!(all(target_os = "linux", feature = "flatpak"))
        );
    }

    #[cfg(feature = "flatpak")]
    #[test]
    fn smartcard_permission_probe_reports_its_result() {
        assert!(super::detect_smartcard_permission_with(|| true));
        assert!(!super::detect_smartcard_permission_with(|| false));
    }
}
