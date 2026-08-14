//! FIDO transport availability and sandbox permission probes.

#[cfg(feature = "flatpak")]
use std::fs;
#[cfg(feature = "flatpak")]
use std::sync::OnceLock;

pub const fn security_key_available() -> bool {
    cfg!(feature = "native-transport")
}

#[cfg(feature = "flatpak")]
pub fn has_usb_permission() -> bool {
    static USB_PERMISSION: OnceLock<bool> = OnceLock::new();

    security_key_available() && *USB_PERMISSION.get_or_init(detect_usb_permission)
}

#[cfg(not(feature = "flatpak"))]
pub const fn has_usb_permission() -> bool {
    security_key_available()
}

#[cfg(feature = "flatpak")]
fn detect_usb_permission() -> bool {
    detect_usb_permission_with(flatpak_usb_device_probe)
}

#[cfg(feature = "flatpak")]
fn detect_usb_permission_with(probe: impl FnOnce() -> bool) -> bool {
    probe()
}

#[cfg(feature = "flatpak")]
fn flatpak_usb_device_probe() -> bool {
    flatpak_context_list("/.flatpak-info", "Context", "devices")
        .is_some_and(|devices| devices.iter().any(|entry| entry == "all"))
}

#[cfg(feature = "flatpak")]
fn flatpak_context_list(path: &str, section: &str, key: &str) -> Option<Vec<String>> {
    let contents = fs::read_to_string(path).ok()?;
    parse_flatpak_context_list(&contents, section, key)
}

#[cfg(feature = "flatpak")]
fn parse_flatpak_context_list(contents: &str, section: &str, key: &str) -> Option<Vec<String>> {
    let mut in_requested_section = false;
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_requested_section = &line[1..line.len() - 1] == section;
            continue;
        }

        if !in_requested_section {
            continue;
        }

        let Some((found_key, value)) = line.split_once('=') else {
            continue;
        };
        if found_key.trim() != key {
            continue;
        }

        return Some(
            value
                .split(';')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToOwned::to_owned)
                .collect(),
        );
    }

    None
}

#[cfg(test)]
mod tests {
    use super::security_key_available;

    #[test]
    fn availability_matches_the_native_transport_feature() {
        assert_eq!(security_key_available(), cfg!(feature = "native-transport"));
    }

    #[cfg(feature = "flatpak")]
    #[test]
    fn usb_permission_probe_reports_its_result() {
        assert!(super::detect_usb_permission_with(|| true));
        assert!(!super::detect_usb_permission_with(|| false));
    }

    #[cfg(feature = "flatpak")]
    #[test]
    fn flatpak_context_list_reads_device_permissions() {
        let contents = "[Context]\ndevices=dri;all;\n";
        assert_eq!(
            super::parse_flatpak_context_list(contents, "Context", "devices"),
            Some(vec!["dri".to_string(), "all".to_string()])
        );
    }
}
