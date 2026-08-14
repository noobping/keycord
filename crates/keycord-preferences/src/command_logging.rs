use keycord_runtime::CommandLogOptions;

const PASSWORD_STORE_ENVIRONMENT_KEYS: &[&str] = &[
    "PASSWORD_STORE_DIR",
    "PASSWORD_STORE_ENABLE_EXTENSIONS",
    "PASSWORD_STORE_EXTENSIONS_DIR",
];

/// Opt into logging the password-store environment configured by Preferences.
///
/// Runtime hides every environment value by default; this owner-level policy exposes
/// only the three configuration keys deliberately included in host-command diagnostics.
pub const fn password_store_command_log_options(
    mut options: CommandLogOptions,
) -> CommandLogOptions {
    options.safe_environment_keys = PASSWORD_STORE_ENVIRONMENT_KEYS;
    options
}

#[cfg(test)]
mod tests {
    use super::password_store_command_log_options;
    use keycord_runtime::CommandLogOptions;

    #[test]
    fn password_store_environment_logging_is_an_explicit_preferences_policy() {
        let options = password_store_command_log_options(CommandLogOptions::DEFAULT);

        assert_eq!(
            options.safe_environment_keys,
            &[
                "PASSWORD_STORE_DIR",
                "PASSWORD_STORE_ENABLE_EXTENSIONS",
                "PASSWORD_STORE_EXTENSIONS_DIR",
            ]
        );
        assert!(options.safe_environment_prefixes.is_empty());
    }
}
