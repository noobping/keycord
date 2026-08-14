//! In-process diagnostic logging with credential redaction.

#[cfg(feature = "logging")]
mod enabled {
    use regex::{Captures, Regex};
    use std::sync::{OnceLock, RwLock};
    use url::Url;

    #[derive(Debug, Default)]
    struct LogState {
        text: String,
        revision: usize,
        error_revision: usize,
    }

    fn global_log_state() -> &'static RwLock<LogState> {
        static LOG_STATE: OnceLock<RwLock<LogState>> = OnceLock::new();
        LOG_STATE.get_or_init(|| RwLock::new(LogState::default()))
    }

    fn with_log_state_read<T>(f: impl FnOnce(&LogState) -> T) -> T {
        match global_log_state().read() {
            Ok(state) => f(&state),
            Err(poisoned) => f(&poisoned.into_inner()),
        }
    }

    fn with_log_state_write<T>(f: impl FnOnce(&mut LogState) -> T) -> T {
        match global_log_state().write() {
            Ok(mut state) => f(&mut state),
            Err(poisoned) => f(&mut poisoned.into_inner()),
        }
    }

    fn push_log_entry(level: &str, message: &str, is_error: bool) {
        let message = sanitize_diagnostic_message(message.trim_end());
        if message.is_empty() {
            return;
        }

        with_log_state_write(|state| {
            if !state.text.is_empty() {
                state.text.push_str("\n\n");
            }
            state.text.push('[');
            state.text.push_str(level);
            state.text.push_str("] ");
            state.text.push_str(&message);
            state.revision += 1;
            if is_error {
                state.error_revision = state.revision;
            }
        });
    }

    pub(super) fn sanitize_diagnostic_message(message: &str) -> String {
        replace_embedded_nuls(&redact_scp_like_credentials(&redact_url_credentials(
            message,
        )))
    }

    fn replace_embedded_nuls(message: &str) -> String {
        message.replace('\0', "\u{FFFD}")
    }

    fn redact_url_credentials(message: &str) -> String {
        credential_url_regex()
            .replace_all(message, |captures: &Captures| {
                let prefix = captures.name("prefix").map_or("", |value| value.as_str());
                let url = captures.name("url").map_or("", |value| value.as_str());
                format!("{prefix}{}", redact_url_credential_value(url))
            })
            .into_owned()
    }

    fn redact_scp_like_credentials(message: &str) -> String {
        scp_remote_regex()
            .replace_all(message, |captures: &Captures| {
                let prefix = captures.name("prefix").map_or("", |value| value.as_str());
                let host = captures.name("host").map_or("", |value| value.as_str());
                let path = captures.name("path").map_or("", |value| value.as_str());
                format!("{prefix}redacted@{host}:{path}")
            })
            .into_owned()
    }

    fn redact_url_credential_value(url: &str) -> String {
        let (url, suffix) = split_trailing_punctuation(url);
        let Ok(mut parsed) = Url::parse(url) else {
            return format!("{url}{suffix}");
        };
        if parsed.username().is_empty() && parsed.password().is_none() {
            return format!("{url}{suffix}");
        }
        if parsed.set_username("redacted").is_err() || parsed.set_password(None).is_err() {
            return format!("{url}{suffix}");
        }

        format!("{}{suffix}", parsed.as_str())
    }

    fn split_trailing_punctuation(value: &str) -> (&str, &str) {
        let mut end = value.len();
        while end > 0 {
            let Some(ch) = value[..end].chars().next_back() else {
                break;
            };
            if !matches!(ch, '.' | ',' | ';' | ')' | ']' | '}') {
                break;
            }
            end -= ch.len_utf8();
        }

        value.split_at(end)
    }

    fn credential_url_regex() -> &'static Regex {
        static REGEX: OnceLock<Regex> = OnceLock::new();
        REGEX.get_or_init(|| {
            Regex::new(r#"(?P<prefix>^|[\s'"(])(?P<url>[A-Za-z][A-Za-z0-9+.-]*://[^\s'"<>]+)"#)
                .expect("credential URL regex should compile")
        })
    }

    fn scp_remote_regex() -> &'static Regex {
        static REGEX: OnceLock<Regex> = OnceLock::new();
        REGEX.get_or_init(|| {
            Regex::new(
                r#"(?P<prefix>^|[\s'"(])[^@\s'"/:]+@(?P<host>[A-Za-z0-9._-]+):(?P<path>[^\s'"<>]*/[^\s'"<>]+)"#,
            )
            .expect("scp remote regex should compile")
        })
    }

    pub(super) fn log_info(message: String) {
        push_log_entry("INFO", &message, false);
    }

    pub(super) fn log_error(message: String) {
        push_log_entry("ERROR", &message, true);
    }

    pub(super) fn log_snapshot() -> (usize, usize, String) {
        with_log_state_read(|state| (state.revision, state.error_revision, state.text.clone()))
    }
}

/// Appends an informational diagnostic when the `logging` feature is enabled.
pub fn log_info(message: impl Into<String>) {
    #[cfg(feature = "logging")]
    enabled::log_info(message.into());
    #[cfg(not(feature = "logging"))]
    let _ = message.into();
}

/// Appends an error diagnostic when the `logging` feature is enabled.
pub fn log_error(message: impl Into<String>) {
    #[cfg(feature = "logging")]
    enabled::log_error(message.into());
    #[cfg(not(feature = "logging"))]
    let _ = message.into();
}

/// Returns `(revision, last_error_revision, text)` for the current log.
pub fn log_snapshot() -> (usize, usize, String) {
    #[cfg(feature = "logging")]
    return enabled::log_snapshot();
    #[cfg(not(feature = "logging"))]
    return (0, 0, String::new());
}

#[cfg(all(test, feature = "logging"))]
mod tests {
    use super::enabled::sanitize_diagnostic_message;

    #[test]
    fn credentialed_urls_are_redacted() {
        let message = sanitize_diagnostic_message(
            "git clone https://user:secret@example.test/private/repo.git",
        );

        assert_eq!(
            message,
            "git clone https://redacted@example.test/private/repo.git"
        );
        assert!(!message.contains("secret"));
    }

    #[test]
    fn scp_like_remotes_are_redacted() {
        let message = sanitize_diagnostic_message("git clone token@example.test:owner/repo.git");

        assert_eq!(message, "git clone redacted@example.test:owner/repo.git");
        assert!(!message.contains("token@"));
    }

    #[test]
    fn embedded_nuls_are_replaced() {
        assert_eq!(
            sanitize_diagnostic_message("alpha\0beta"),
            "alpha\u{FFFD}beta"
        );
    }
}
