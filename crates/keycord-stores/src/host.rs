//! Host `pass` backend orchestration for store recipients.

use std::path::Path;
use std::process::{Command, Output};

use crate::error::store_recipients_error_from_integrated_message;
use crate::path_validation::validated_relative_directory_path;
use crate::{StoreRecipients, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement};

pub const NESTED_RECIPIENTS_REQUIRE_INTEGRATED_BACKEND: &str =
    "Managing nested .gpg-id files requires the Integrated backend.";

pub trait HostStorePorts {
    fn try_initialize_empty_store_recipients(
        &self,
        store_root: &str,
        recipients: &StoreRecipients,
        private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    ) -> Result<bool, String>;

    fn run_pass_init(&self, store_root: &str, recipients: &[String]) -> Result<Output, String>;

    fn has_git_repository(&self, store_root: &str) -> bool;

    fn ensure_git_repository(&self, store_root: &str) -> Result<(), String>;
}

pub fn configure_pass_init_command(cmd: &mut Command, recipients: &[String]) {
    cmd.arg("init").arg("--");
    cmd.args(recipients);
}

fn host_failure_message(output: &Output, fallback_prefix: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("{fallback_prefix}: {}", output.status)
    }
}

pub fn store_recipients_error_from_host_message(
    message: impl Into<String>,
) -> StoreRecipientsError {
    let message = message.into();
    let lowered = message.to_ascii_lowercase();
    if lowered.contains("selected password store path is not a folder") {
        StoreRecipientsError::invalid_store_path(message)
    } else if message.contains("Import a private key in Preferences") {
        StoreRecipientsError::MissingPrivateKey(message)
    } else if message.contains("A private key for this item is locked.") {
        StoreRecipientsError::LockedPrivateKey(message)
    } else if lowered.contains("cannot decrypt password store entries")
        || lowered.contains("available private keys cannot decrypt")
        || lowered.contains("no pkesks managed to decrypt the ciphertext")
        || lowered.contains("no pkesk managed to decrypt the ciphertext")
    {
        StoreRecipientsError::IncompatiblePrivateKey(message)
    } else {
        StoreRecipientsError::other(message)
    }
}

pub fn save_store_recipients_with<P: HostStorePorts + ?Sized>(
    ports: &P,
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    if ports
        .try_initialize_empty_store_recipients(store_root, recipients, private_key_requirement)
        .map_err(store_recipients_error_from_integrated_message)?
    {
        return Ok(());
    }

    let should_initialize_git =
        !Path::new(store_root).join(".gpg-id").exists() && !ports.has_git_repository(store_root);
    let output = ports
        .run_pass_init(store_root, recipients.standard())
        .map_err(StoreRecipientsError::other)?;
    if !output.status.success() {
        return Err(store_recipients_error_from_host_message(
            host_failure_message(&output, "pass init failed"),
        ));
    }

    if should_initialize_git {
        ports
            .ensure_git_repository(store_root)
            .map_err(StoreRecipientsError::other)?;
    }
    Ok(())
}

pub fn save_store_recipients_for_relative_dir_with<P: HostStorePorts + ?Sized>(
    ports: &P,
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    let relative_dir =
        validated_relative_directory_path(relative_dir).map_err(StoreRecipientsError::other)?;
    if relative_dir.as_os_str().is_empty() {
        return save_store_recipients_with(ports, store_root, recipients, private_key_requirement);
    }

    Err(StoreRecipientsError::other(
        NESTED_RECIPIENTS_REQUIRE_INTEGRATED_BACKEND,
    ))
}

pub const fn store_recipients_private_key_requiring_unlock(_store_root: &str) -> Option<String> {
    None
}

pub fn store_recipients_private_key_requiring_unlock_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
) -> Result<Option<String>, String> {
    let relative_dir = validated_relative_directory_path(relative_dir)?;
    if relative_dir.as_os_str().is_empty() {
        return Ok(store_recipients_private_key_requiring_unlock(store_root));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::process::{Command, ExitStatus, Output};

    use super::{configure_pass_init_command, store_recipients_error_from_host_message};
    use crate::StoreRecipientsError;

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    #[test]
    fn pass_init_terminates_option_parsing_for_recipients() {
        let mut command = Command::new("pass");
        configure_pass_init_command(
            &mut command,
            &["-recipient".to_string(), "ABCD".to_string()],
        );
        assert_eq!(
            command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>(),
            ["init", "--", "-recipient", "ABCD"]
        );
    }

    #[test]
    fn host_recipient_messages_map_to_specific_errors() {
        assert!(matches!(
            store_recipients_error_from_host_message(
                "Import a private key in Preferences before using the password store."
            ),
            StoreRecipientsError::MissingPrivateKey(_)
        ));
        assert!(matches!(
            store_recipients_error_from_host_message(
                "The selected password store path is not a folder."
            ),
            StoreRecipientsError::InvalidStorePath(_)
        ));
    }

    #[test]
    fn output_status_can_be_constructed_for_host_port_tests() {
        let output = Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        assert!(!output.status.success());
    }
}
