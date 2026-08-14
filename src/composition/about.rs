//! Builds application About details from the selected backend adapters.

use keycord_preferences::{password_store_command_log_options, Preferences};
use keycord_runtime::i18n::gettext;
use keycord_runtime::{run_command_output, CommandLogOptions};

const RIPASSO_VERSION: &str = env!("RIPASSO_VERSION");
const SEQUOIA_OPENPGP_VERSION: &str = env!("SEQUOIA_OPENPGP_VERSION");

pub fn comments(project: &str) -> String {
    let comments = gettext(option_env!("CARGO_PKG_DESCRIPTION").unwrap_or(""));
    let settings = Preferences::new();
    let backend_details = if settings.uses_integrated_backend() {
        format!(
            "{} {RIPASSO_VERSION}\n{} {SEQUOIA_OPENPGP_VERSION}",
            gettext("backend: ripasso"),
            gettext("sequoia-openpgp")
        )
    } else {
        host_password_store_version(&settings).map_or_else(
            || gettext("backend: host"),
            |version| format!("{}\n{version}", gettext("backend: host")),
        )
    };

    if comments.is_empty() {
        backend_details
    } else {
        format!("{project}: {comments}\n\n{backend_details}")
    }
}

fn host_password_store_version(settings: &Preferences) -> Option<String> {
    let mut command = settings.command();
    command.arg("--version");
    let output = run_command_output(
        &mut command,
        "Read pass version",
        password_store_command_log_options(CommandLogOptions::DEFAULT),
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_host_password_store_version(&output.stdout)
}

fn parse_host_password_store_version(output: &[u8]) -> Option<String> {
    let lines = String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .map(|line| line.trim_matches('='))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::parse_host_password_store_version;

    #[test]
    fn host_backend_version_ignores_banner_decoration_and_blank_lines() {
        assert_eq!(
            parse_host_password_store_version(b"\n== pass 1.7.4 ==\nCopyright 2012\n"),
            Some("pass 1.7.4\nCopyright 2012".to_string())
        );
        assert_eq!(parse_host_password_store_version(b"\n===\n"), None);
    }
}
