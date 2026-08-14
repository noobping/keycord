//! Generated desktop and search-provider install assets.

use crate::desktop::{
    desktop_file, search_provider_bus_name, search_provider_file, search_provider_object_path,
    search_provider_service_file, PasskeyMimeConfig,
};
use std::fs;
use std::io;
use std::path::Path;

pub fn write_install_assets(
    dir: &Path,
    app_id: &str,
    executable: &str,
    display_name: &str,
    comment: &str,
    passkey_mime: Option<PasskeyMimeConfig<'_>>,
) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let bus_name = search_provider_bus_name(app_id);
    let object_path = search_provider_object_path(&bus_name);

    fs::write(
        dir.join(format!("{executable}.desktop")),
        desktop_file(app_id, executable, display_name, comment, passkey_mime),
    )?;
    fs::write(
        dir.join(format!("{executable}-search-provider.ini")),
        search_provider_file(app_id, &bus_name, &object_path),
    )?;
    fs::write(
        dir.join(format!("{executable}-search-provider.service")),
        search_provider_service_file(&bus_name, executable),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("keycord-{label}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn install_assets_use_explicit_optional_mime_metadata() {
        let enabled = temporary_directory("build-assets-mime-enabled");
        write_install_assets(
            &enabled,
            "io.github.example.App",
            "example",
            "Example",
            "Example application",
            Some(PasskeyMimeConfig {
                mime_types: "application/vnd.example.passkey+json;",
                package: "<mime-info />",
            }),
        )
        .expect("write enabled install assets");
        let enabled_desktop =
            fs::read_to_string(enabled.join("example.desktop")).expect("read enabled desktop");
        assert!(enabled_desktop.contains("Exec=example %f\n"));
        assert!(enabled_desktop.contains("MimeType=application/vnd.example.passkey+json;\n"));

        let disabled = temporary_directory("build-assets-mime-disabled");
        write_install_assets(
            &disabled,
            "io.github.example.App",
            "example",
            "Example",
            "Example application",
            None,
        )
        .expect("write disabled install assets");
        let disabled_desktop =
            fs::read_to_string(disabled.join("example.desktop")).expect("read disabled desktop");
        assert!(disabled_desktop.contains("Exec=example\n"));
        assert!(!disabled_desktop.contains("MimeType="));

        let _ = fs::remove_dir_all(enabled);
        let _ = fs::remove_dir_all(disabled);
    }
}
