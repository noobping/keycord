use std::path::{Path, PathBuf};

// Preserve Keycord's established on-disk data directory after moving this code
// into a subject crate with a different Cargo package name.
const APP_DATA_COMPONENT: &str = "keycord";

pub fn ripasso_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(APP_DATA_COMPONENT).join("keys"))
}

pub(super) fn ripasso_keys_v2_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(APP_DATA_COMPONENT).join("keys-v2"))
}

#[cfg(feature = "fido")]
pub(super) fn ripasso_fido_keys_dir() -> Result<PathBuf, String> {
    let data_dir = dirs_next::data_local_dir()
        .ok_or_else(|| "Could not determine the data folder.".to_string())?;
    Ok(data_dir.join(APP_DATA_COMPONENT).join("keys-fido"))
}

pub(super) fn hardware_manifest_path(dir: &Path) -> PathBuf {
    dir.join("manifest.toml")
}

pub(super) fn hardware_public_key_path(dir: &Path) -> PathBuf {
    dir.join("public.asc")
}
