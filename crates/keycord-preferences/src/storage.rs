use super::{PasswordListSortMode, UsernameFallbackMode};
use crate::PasswordGenerationSettings;
use glib::{bool_error, BoolError};
use keycord_runtime::bounded_toml::{parse_toml_with_limits, TomlParseLimits};
use keycord_runtime::secure_fs::write_private_file;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const PREFERENCE_FILE_TOML_LIMITS: TomlParseLimits = TomlParseLimits::new(64 * 1024, 16);

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(super) struct PreferenceFile {
    pub(super) backend: Option<String>,
    pub(super) pass_command: Option<String>,
    pub(super) password_store_dirs: Option<Vec<String>>,
    pub(super) window_width: Option<i32>,
    pub(super) window_height: Option<i32>,
    pub(super) new_pass_file_template: Option<String>,
    pub(super) clear_empty_fields_before_save: Option<bool>,
    pub(super) password_generation: Option<PasswordGenerationSettings>,
    pub(super) username_fallback_mode: Option<UsernameFallbackMode>,
    pub(super) password_list_sort_mode: Option<PasswordListSortMode>,
    pub(super) ripasso_own_fingerprint: Option<String>,
    pub(super) sync_private_keys_with_host: Option<bool>,
    pub(super) audit_use_commit_history_recipients: Option<bool>,
    pub(super) translation_help_notification_shown: Option<bool>,
    pub(super) filter_included_store_roots: Option<Vec<String>>,
    pub(super) audit_filter_included_branches: Option<Vec<String>>,
    pub(super) hidden_notices: Option<Vec<String>>,
}

fn config_path() -> PathBuf {
    dirs_next::config_dir().map_or_else(
        || PathBuf::from("keycord.toml"),
        |dir| dir.join("keycord.toml"),
    )
}

pub(super) fn load_file_prefs() -> PreferenceFile {
    let path = config_path();
    fs::read_to_string(&path).map_or_else(
        |_| PreferenceFile::default(),
        |data| {
            parse_toml_with_limits(&data, PREFERENCE_FILE_TOML_LIMITS, "preferences file")
                .unwrap_or_default()
        },
    )
}

pub(super) fn save_file_prefs(cfg: &PreferenceFile) -> Result<(), BoolError> {
    let path = config_path();

    let toml =
        toml::to_string_pretty(cfg).map_err(|e| bool_error!("Failed to serialize config: {e}"))?;

    write_private_file(&path, toml.as_bytes())
        .map_err(|e| bool_error!("Failed to write config file: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{config_path, PreferenceFile, PREFERENCE_FILE_TOML_LIMITS};
    use keycord_runtime::bounded_toml::parse_toml_with_limits;

    #[test]
    fn fallback_filename_keeps_the_product_name() {
        assert_eq!(
            config_path().file_name().and_then(|name| name.to_str()),
            Some("keycord.toml")
        );
    }

    #[test]
    fn preference_file_limit_is_owned_by_preferences() {
        let oversized = "x".repeat(PREFERENCE_FILE_TOML_LIMITS.max_bytes + 1);
        let error = parse_toml_with_limits::<PreferenceFile>(
            &oversized,
            PREFERENCE_FILE_TOML_LIMITS,
            "preferences file",
        )
        .expect_err("oversized preferences must be rejected");

        assert!(error.contains("size limit"));
    }
}
