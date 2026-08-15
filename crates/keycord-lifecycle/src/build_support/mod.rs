//! Build-time orchestration for the Keycord application package.

pub use crate::desktop::{
    app_id, desktop_file, search_provider_bus_name, search_provider_file,
    search_provider_object_path, search_provider_service_file, PasskeyMimeConfig,
    DEVELOPMENT_APP_ID, GETTEXT_DOMAIN, PRODUCT_DESCRIPTION, PRODUCT_NAME, RELEASE_APP_ID,
    RESOURCE_ID,
};
use std::fs;
use std::path::Path;

mod assets;
mod metadata;
mod pipeline;
mod resources;
mod translations;
#[cfg(target_os = "windows")]
mod windows;
mod workspace_data;

pub use assets::write_install_assets;
pub use pipeline::run_application_build;

fn write_if_changed(path: &Path, contents: impl AsRef<[u8]>) {
    let contents = contents.as_ref();
    if fs::read(path).ok().as_deref() == Some(contents) {
        return;
    }

    fs::write(path, contents)
        .unwrap_or_else(|err| panic!("Failed to write {}: {err}", path.display()));
}

#[derive(Clone, Copy, Debug)]
pub struct ApplicationBuildConfig<'a> {
    pub source_root: &'a Path,
    pub out_dir: &'a Path,
    pub package_name: &'a str,
    pub package_version: &'a str,
    pub package_description: &'a str,
    pub display_name: &'a str,
    pub debug: bool,
    pub flatpak: bool,
    pub passkey_mime: Option<PasskeyMimeConfig<'a>>,
    pub setup: bool,
    pub target_os: Option<&'a str>,
    pub target_env: Option<&'a str>,
}
