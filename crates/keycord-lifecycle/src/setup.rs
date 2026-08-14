use gio::{self, ResourceLookupFlags};
use std::fs;
use std::io::{Error, ErrorKind};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::{
    process,
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(feature = "ui")]
use std::sync::OnceLock;

use crate::desktop::{
    desktop_file, search_provider_file, search_provider_service_file, PasskeyMimeConfig,
};

#[cfg(feature = "ui")]
use adw::gtk::ListBox;
#[cfg(feature = "ui")]
use adw::prelude::PreferencesRowExt;
#[cfg(feature = "ui")]
use adw::{ActionRow, Toast, ToastOverlay};
#[cfg(feature = "ui")]
use keycord_runtime::{i18n::gettext, log_error};
#[cfg(feature = "ui")]
use keycord_shell::ui::append_action_row_with_button;

#[derive(Clone, Copy, Debug)]
pub struct InstallConfig {
    pub product_name: &'static str,
    pub product_description: &'static str,
    pub app_id: &'static str,
    pub gettext_domain: &'static str,
    pub locale_dir: &'static str,
    pub available_locales: &'static str,
    pub resource_id: &'static str,
    pub search_provider_bus_name: &'static str,
    pub search_provider_object_path: &'static str,
    pub passkey_mime: Option<PasskeyMimeConfig<'static>>,
    pub initialize_i18n: fn(),
    pub register_resources: fn() -> Result<(), String>,
}

#[cfg(feature = "ui")]
static INSTALL_CONFIG: OnceLock<InstallConfig> = OnceLock::new();

#[cfg(feature = "ui")]
pub(crate) fn configure(config: InstallConfig) {
    let _ = INSTALL_CONFIG.set(config);
}

#[cfg(all(
    feature = "ui",
    target_os = "linux",
    feature = "setup",
    not(feature = "flatpak")
))]
pub(crate) fn configured() -> &'static InstallConfig {
    INSTALL_CONFIG
        .get()
        .expect("lifecycle install configuration must be initialized")
}

pub fn local_menu_action_label(installed: bool) -> &'static str {
    if installed {
        "Remove from app menu"
    } else {
        "Add to app menu"
    }
}

#[cfg(feature = "ui")]
pub fn append_local_install_row(
    list: &ListBox,
    overlay: &ToastOverlay,
    config: InstallConfig,
    on_changed: impl Fn() + 'static,
) -> Option<ActionRow> {
    if !can_install_locally(&config) {
        return None;
    }

    let overlay = overlay.clone();
    let row = append_action_row_with_button(
        list,
        local_menu_action_label(is_installed_locally(&config)),
        "Add or remove this build from the local app menu.",
        "emblem-system-symbolic",
        move || {
            let result = if is_installed_locally(&config) {
                uninstall_locally(&config)
            } else {
                install_locally(&config)
            };

            match result {
                Ok(()) => on_changed(),
                Err(err) => {
                    log_error(format!("Failed to update local app menu entry: {err}"));
                    overlay.add_toast(Toast::new(&gettext("Couldn't update the app menu.")));
                }
            }
        },
    );
    Some(row)
}

#[cfg(feature = "ui")]
pub fn sync_local_install_row(row: Option<&ActionRow>, config: &InstallConfig) {
    let Some(row) = row else {
        return;
    };

    row.set_title(&gettext(local_menu_action_label(is_installed_locally(
        config,
    ))));
}

pub fn can_install_locally(config: &InstallConfig) -> bool {
    let Some(bin) = dirs_next::executable_dir() else {
        return false;
    };
    let Some(data) = dirs_next::data_dir() else {
        return false;
    };

    can_install_into(&bin, &data, config)
}

pub fn is_installed_locally(config: &InstallConfig) -> bool {
    let Some(bin) = installed_local_binary_path(config) else {
        return false;
    };
    let Some(data) = dirs_next::data_dir() else {
        return false;
    };
    let desktop = data
        .join("applications")
        .join(format!("{}.desktop", config.app_id));
    bin.exists() && bin.is_file() && desktop.exists() && desktop.is_file()
}

pub fn is_current_executable_installed_locally(config: &InstallConfig) -> bool {
    let Ok(current_exe) = std::env::current_exe() else {
        return false;
    };
    let Some(installed_exe) = installed_local_binary_path(config) else {
        return false;
    };

    same_file_path(&current_exe, &installed_exe)
}

pub fn installed_local_binary_path(config: &InstallConfig) -> Option<PathBuf> {
    let bin = dirs_next::executable_dir()?;

    Some(bin.join(config.product_name))
}

pub fn install_locally(config: &InstallConfig) -> std::io::Result<()> {
    let project = config.product_name;
    let exe_path = std::env::current_exe()?;
    let Some(bin) = dirs_next::executable_dir() else {
        return Err(Error::new(
            ErrorKind::NotFound,
            "No executable directory found",
        ));
    };
    let Some(data) = dirs_next::data_dir() else {
        return Err(Error::new(ErrorKind::NotFound, "No data directory found"));
    };
    let apps = data.join("applications");
    let mime_packages = data.join("mime").join("packages");
    let dbus_services = data.join("dbus-1").join("services");
    let search_providers = data.join("gnome-shell").join("search-providers");
    let icons = data
        .join("icons")
        .join("hicolor")
        .join("scalable")
        .join("apps");
    let locale_root = data.join("locale");
    let dest = bin.join(project);

    if !can_install_into(&bin, &data, config) {
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "One or more local install directories are not writable.",
        ));
    }

    std::fs::create_dir_all(&bin)?;
    std::fs::create_dir_all(&apps)?;
    if config.passkey_mime.is_some() {
        std::fs::create_dir_all(&mime_packages)?;
    }
    std::fs::create_dir_all(&dbus_services)?;
    std::fs::create_dir_all(&search_providers)?;
    std::fs::create_dir_all(&icons)?;
    std::fs::copy(&exe_path, &dest)?;

    let mut perms = std::fs::metadata(&dest)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&dest, perms)?;

    write_desktop_file(&apps, &dest, config)?;
    if let Some(passkey_mime) = config.passkey_mime {
        write_passkey_mime_package(&mime_packages, config.app_id, passkey_mime.package)?;
    }
    write_search_provider_file(&search_providers, config)?;
    write_search_provider_service_file(&dbus_services, &dest, config)?;
    extract_icon(&icons, config)?;
    install_locales(&locale_root, config)?;
    if config.passkey_mime.is_some() {
        refresh_mime_database(&data);
    }

    Ok(())
}

pub fn uninstall_locally(config: &InstallConfig) -> std::io::Result<()> {
    let Some(bin) = dirs_next::executable_dir() else {
        return Err(Error::new(
            ErrorKind::NotFound,
            "No executable directory found",
        ));
    };
    let Some(data) = dirs_next::data_dir() else {
        return Err(Error::new(ErrorKind::NotFound, "No data directory found"));
    };
    let bin = bin.join(config.product_name);
    let icon = data
        .join("icons")
        .join("hicolor")
        .join("scalable")
        .join("apps")
        .join(format!("{}.svg", config.app_id));
    let desktop = data
        .join("applications")
        .join(format!("{}.desktop", config.app_id));
    let passkey_mime = data
        .join("mime")
        .join("packages")
        .join(format!("{}-passkey.xml", config.app_id));
    let search_provider = data
        .join("gnome-shell")
        .join("search-providers")
        .join(format!("{}.search-provider.ini", config.app_id));
    let service = data
        .join("dbus-1")
        .join("services")
        .join(format!("{}.service", config.search_provider_bus_name));
    if bin.exists() {
        fs::remove_file(bin)?;
    }
    if desktop.exists() {
        fs::remove_file(desktop)?;
    }
    if config.passkey_mime.is_some() && passkey_mime.exists() {
        fs::remove_file(passkey_mime)?;
    }
    if search_provider.exists() {
        fs::remove_file(search_provider)?;
    }
    if service.exists() {
        fs::remove_file(service)?;
    }
    if icon.exists() {
        fs::remove_file(icon)?;
    }
    remove_installed_locales(&data.join("locale"), config)?;
    if config.passkey_mime.is_some() {
        refresh_mime_database(&data);
    }
    Ok(())
}

fn can_install_into(bin: &Path, data: &Path, config: &InstallConfig) -> bool {
    let mut targets = vec![
        bin.to_path_buf(),
        data.join("applications"),
        data.join("dbus-1").join("services"),
        data.join("gnome-shell").join("search-providers"),
        data.join("icons")
            .join("hicolor")
            .join("scalable")
            .join("apps"),
    ];
    if config.passkey_mime.is_some() {
        targets.push(data.join("mime").join("packages"));
    }
    if locale_install_required(config) {
        targets.push(data.join("locale"));
    }

    targets
        .iter()
        .all(|target| install_target_dir_is_eligible(target))
}

fn locale_install_required(config: &InstallConfig) -> bool {
    available_locales(config).any(|locale| {
        Path::new(config.locale_dir)
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{}.mo", config.gettext_domain))
            .exists()
    })
}

fn is_writable(dir: &Path) -> bool {
    for attempt in 0..8u32 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let test_path = dir.join(format!(
            ".perm_test.{}.{}.{}",
            process::id(),
            nanos,
            attempt
        ));
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&test_path)
        {
            Ok(_) => {
                let _ = std::fs::remove_file(test_path);
                return true;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return false,
        }
    }

    false
}

fn install_target_dir_is_eligible(path: &Path) -> bool {
    let mut candidate = Some(path);
    while let Some(dir) = candidate {
        if dir.exists() {
            return dir.is_dir() && is_writable(dir);
        }
        candidate = dir.parent();
    }

    false
}

fn same_file_path(left: &Path, right: &Path) -> bool {
    match (fs::canonicalize(left), fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

fn write_desktop_file(
    apps_path: &Path,
    bin_path: &Path,
    config: &InstallConfig,
) -> std::io::Result<()> {
    let contents = desktop_file(
        config.app_id,
        &bin_path.display().to_string(),
        config.product_name,
        config.product_description,
        config.passkey_mime,
    );

    let file = apps_path.join(format!("{}.desktop", config.app_id));
    fs::write(&file, contents)?;

    // Make sure it's readable by the user
    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file, perms)?;

    Ok(())
}

fn write_passkey_mime_package(
    packages_path: &Path,
    app_id: &str,
    package: &str,
) -> std::io::Result<()> {
    let file = packages_path.join(format!("{app_id}-passkey.xml"));
    fs::write(&file, package)?;
    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(file, perms)
}

fn refresh_mime_database(data_path: &Path) {
    let _ = process::Command::new("update-mime-database")
        .arg(data_path.join("mime"))
        .status();
}

fn write_search_provider_file(
    search_providers_path: &Path,
    config: &InstallConfig,
) -> std::io::Result<()> {
    let contents = search_provider_file(
        config.app_id,
        config.search_provider_bus_name,
        config.search_provider_object_path,
    );
    let file = search_providers_path.join(format!("{}.search-provider.ini", config.app_id));
    fs::write(&file, contents)?;

    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file, perms)?;

    Ok(())
}

fn write_search_provider_service_file(
    services_path: &Path,
    bin_path: &Path,
    config: &InstallConfig,
) -> std::io::Result<()> {
    let contents = search_provider_service_file(
        config.search_provider_bus_name,
        &bin_path.display().to_string(),
    );
    let file = services_path.join(format!("{}.service", config.search_provider_bus_name));
    fs::write(&file, contents)?;

    let mut perms = fs::metadata(&file)?.permissions();
    perms.set_mode(0o644);
    fs::set_permissions(&file, perms)?;

    Ok(())
}

fn extract_icon(apps_dir: &Path, config: &InstallConfig) -> std::io::Result<()> {
    let resource_path = format!("{}/scalable/apps/{}.svg", config.resource_id, config.app_id);
    println!("Looking up resource: {resource_path}");
    let bytes = gio::resources_lookup_data(&resource_path, ResourceLookupFlags::NONE)
        .map_err(|e| Error::new(ErrorKind::NotFound, format!("Resource not found: {e}")))?;
    let out_path = apps_dir.join(format!("{}.svg", config.app_id));
    std::fs::write(&out_path, bytes.as_ref())?;
    Ok(())
}

fn install_locales(locale_root: &Path, config: &InstallConfig) -> std::io::Result<()> {
    for locale in available_locales(config) {
        let source = Path::new(config.locale_dir)
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{}.mo", config.gettext_domain));
        if !source.exists() {
            continue;
        }

        let destination = locale_root
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{}.mo", config.gettext_domain));
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination)?;
    }

    Ok(())
}

fn remove_installed_locales(locale_root: &Path, config: &InstallConfig) -> std::io::Result<()> {
    for locale in available_locales(config) {
        let destination = locale_root
            .join(locale)
            .join("LC_MESSAGES")
            .join(format!("{}.mo", config.gettext_domain));
        if destination.exists() {
            fs::remove_file(destination)?;
        }
    }

    Ok(())
}

fn available_locales(config: &InstallConfig) -> impl Iterator<Item = &'static str> {
    config
        .available_locales
        .split(':')
        .filter(|locale| !locale.is_empty())
}

#[cfg(test)]
pub(crate) fn test_install_config() -> InstallConfig {
    fn no_op() {}
    fn register_resources() -> Result<(), String> {
        Ok(())
    }

    InstallConfig {
        product_name: "keycord",
        product_description: "Browse and edit password stores",
        app_id: "io.github.noobping.keycord",
        gettext_domain: "keycord",
        locale_dir: "",
        available_locales: "",
        resource_id: "/io/github/noobping/keycord",
        search_provider_bus_name: "io.github.noobping.keycord.SearchProvider",
        search_provider_object_path: "/io/github/noobping/keycord/SearchProvider",
        passkey_mime: None,
        initialize_i18n: no_op,
        register_resources,
    }
}

#[test]
fn install_outputs_use_supplied_passkey_mime_configuration() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("keycord-setup-passkey-mime-{unique}"));
    let applications = root.join("applications");
    let packages = root.join("mime").join("packages");
    fs::create_dir_all(&applications).expect("create applications dir");
    fs::create_dir_all(&packages).expect("create MIME packages dir");

    let mut config = test_install_config();
    config.passkey_mime = Some(PasskeyMimeConfig {
        mime_types: "application/vnd.example.passkey+json;",
        package: "<mime-info>example</mime-info>\n",
    });
    write_desktop_file(&applications, Path::new("/tmp/keycord"), &config)
        .expect("write desktop file");
    let passkey_mime = config.passkey_mime.expect("passkey MIME config");
    write_passkey_mime_package(&packages, config.app_id, passkey_mime.package)
        .expect("write MIME package");

    let desktop = fs::read_to_string(applications.join("io.github.noobping.keycord.desktop"))
        .expect("read desktop file");
    assert!(desktop.contains("Exec=/tmp/keycord %f\n"));
    assert!(desktop.contains("MimeType=application/vnd.example.passkey+json;\n"));
    assert_eq!(
        fs::read_to_string(packages.join("io.github.noobping.keycord-passkey.xml"))
            .expect("read MIME package"),
        "<mime-info>example</mime-info>\n"
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn writability_probe_does_not_truncate_existing_perm_test_files() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("keycord-setup-writable-{unique}"));
    fs::create_dir_all(&dir).expect("create temp dir");
    let existing = dir.join(".perm_test");
    fs::write(&existing, "keep").expect("write marker");

    assert!(is_writable(&dir));
    assert_eq!(
        fs::read_to_string(&existing).expect("read marker"),
        "keep".to_string()
    );

    let _ = fs::remove_dir_all(dir);
}

#[test]
fn install_target_dir_rejects_existing_non_writable_directories() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("keycord-setup-existing-dir-{unique}"));
    let target = root.join("applications");
    fs::create_dir_all(&target).expect("create target dir");
    let mut permissions = fs::metadata(&target)
        .expect("read target metadata")
        .permissions();
    permissions.set_mode(0o500);
    fs::set_permissions(&target, permissions).expect("make target non-writable");

    assert!(!install_target_dir_is_eligible(&target));

    let mut cleanup_permissions = fs::metadata(&target)
        .expect("read target metadata for cleanup")
        .permissions();
    cleanup_permissions.set_mode(0o700);
    fs::set_permissions(&target, cleanup_permissions).expect("restore target permissions");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_target_dir_accepts_missing_nested_directories_under_writable_ancestor() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("keycord-setup-missing-dir-{unique}"));
    fs::create_dir_all(&root).expect("create root dir");

    assert!(install_target_dir_is_eligible(
        &root
            .join("icons")
            .join("hicolor")
            .join("scalable")
            .join("apps")
    ));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn install_eligibility_checks_all_written_target_directories() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("keycord-setup-eligibility-{unique}"));
    let bin = root.join("bin");
    let data = root.join("data");
    let search_providers = data.join("gnome-shell").join("search-providers");
    fs::create_dir_all(&bin).expect("create bin dir");
    fs::create_dir_all(&search_providers).expect("create search provider dir");
    let mut permissions = fs::metadata(&search_providers)
        .expect("read search provider metadata")
        .permissions();
    permissions.set_mode(0o500);
    fs::set_permissions(&search_providers, permissions).expect("make search provider non-writable");

    assert!(!can_install_into(&bin, &data, &test_install_config()));

    let mut cleanup_permissions = fs::metadata(&search_providers)
        .expect("read search provider metadata for cleanup")
        .permissions();
    cleanup_permissions.set_mode(0o700);
    fs::set_permissions(&search_providers, cleanup_permissions)
        .expect("restore search provider permissions");
    let _ = fs::remove_dir_all(root);
}
