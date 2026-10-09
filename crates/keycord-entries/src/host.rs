//! Host `pass` backend operations for password entries.

use crate::{PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError};
use keycord_runtime::CommandLogOptions;
use keycord_stores::path_validation::validated_entry_label_path;
use std::process::{Command, Output};

/// Supplies the configured host command and process runner without coupling
/// Entries to application preferences or root logging composition.
pub trait HostEntryCommandPort: Send + Sync {
    fn run_store_command_output(
        &self,
        store_root: &str,
        action: &str,
        log_options: CommandLogOptions,
        configure: &mut dyn FnMut(&mut Command),
    ) -> Result<Output, String>;

    fn run_store_command_with_input(
        &self,
        store_root: &str,
        action: &str,
        input: &str,
        log_options: CommandLogOptions,
        configure: &mut dyn FnMut(&mut Command),
    ) -> Result<Output, String>;
    fn run_store_command_with_bytes(
        &self,
        store_root: &str,
        action: &str,
        input: &[u8],
        log_options: CommandLogOptions,
        configure: &mut dyn FnMut(&mut Command),
    ) -> Result<Output, String> {
        let text = std::str::from_utf8(input)
            .map_err(|_| "This backend cannot write binary entries.".to_string())?;
        self.run_store_command_with_input(store_root, action, text, log_options, configure)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostStoreAction {
    ReadEntry,
    ReadLine,
    SaveEntry,
    RenameEntry,
    DeleteEntry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HostCommandFailure {
    action: HostStoreAction,
    message: String,
}

impl HostCommandFailure {
    fn from_output(action: HostStoreAction, output: Output, fallback_prefix: &str) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() && !action.stdout_may_contain_secret_data() {
            stdout
        } else {
            format!("{fallback_prefix}: {}", output.status)
        };

        Self { action, message }
    }

    fn message(&self) -> &str {
        &self.message
    }
}

impl HostStoreAction {
    const fn stdout_may_contain_secret_data(self) -> bool {
        matches!(self, Self::ReadEntry | Self::ReadLine | Self::SaveEntry)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostStoreFailureKind {
    EntryNotFound,
    EntryAlreadyExists,
    MissingPrivateKey,
    LockedPrivateKey,
    IncompatiblePrivateKey,
    Other,
}

fn message_contains_any(lowered: &str, patterns: &[&str]) -> bool {
    patterns.iter().any(|pattern| lowered.contains(pattern))
}

fn host_failure_is_entry_not_found(action: HostStoreAction, lowered: &str) -> bool {
    matches!(
        action,
        HostStoreAction::ReadEntry
            | HostStoreAction::ReadLine
            | HostStoreAction::RenameEntry
            | HostStoreAction::DeleteEntry
    ) && message_contains_any(
        lowered,
        &[
            "not in the password store",
            "was not found",
            "no such file or directory",
        ],
    )
}

fn host_failure_is_already_exists(action: HostStoreAction, lowered: &str) -> bool {
    matches!(
        action,
        HostStoreAction::SaveEntry | HostStoreAction::RenameEntry
    ) && lowered.contains("already exists")
}

fn host_failure_is_incompatible_private_key(message: &str) -> bool {
    message.contains("cannot decrypt password store entries")
        || message.contains("available private keys cannot decrypt")
        || message.contains("no pkesks managed to decrypt the ciphertext")
        || message.contains("no pkesk managed to decrypt the ciphertext")
}

fn classify_host_store_failure(failure: &HostCommandFailure) -> HostStoreFailureKind {
    let lowered = failure.message().to_ascii_lowercase();
    if host_failure_is_entry_not_found(failure.action, &lowered) {
        HostStoreFailureKind::EntryNotFound
    } else if host_failure_is_already_exists(failure.action, &lowered) {
        HostStoreFailureKind::EntryAlreadyExists
    } else if failure
        .message()
        .contains("Import a private key in Preferences")
    {
        HostStoreFailureKind::MissingPrivateKey
    } else if failure
        .message()
        .contains("A private key for this item is locked.")
    {
        HostStoreFailureKind::LockedPrivateKey
    } else if host_failure_is_incompatible_private_key(failure.message()) {
        HostStoreFailureKind::IncompatiblePrivateKey
    } else {
        HostStoreFailureKind::Other
    }
}

fn ensure_host_command_success(
    action: HostStoreAction,
    output: Output,
    fallback_prefix: &str,
) -> Result<Output, HostCommandFailure> {
    if output.status.success() {
        Ok(output)
    } else {
        Err(HostCommandFailure::from_output(
            action,
            output,
            fallback_prefix,
        ))
    }
}

fn password_entry_error_from_host_failure(failure: HostCommandFailure) -> PasswordEntryError {
    let kind = classify_host_store_failure(&failure);
    let message = failure.message;
    match kind {
        HostStoreFailureKind::EntryNotFound => PasswordEntryError::EntryNotFound(message),
        HostStoreFailureKind::MissingPrivateKey => PasswordEntryError::MissingPrivateKey(message),
        HostStoreFailureKind::LockedPrivateKey => PasswordEntryError::LockedPrivateKey(message),
        HostStoreFailureKind::IncompatiblePrivateKey => {
            PasswordEntryError::IncompatiblePrivateKey(message)
        }
        HostStoreFailureKind::EntryAlreadyExists | HostStoreFailureKind::Other => {
            PasswordEntryError::other(message)
        }
    }
}

fn password_entry_write_error_from_host_failure(
    failure: HostCommandFailure,
) -> PasswordEntryWriteError {
    let kind = classify_host_store_failure(&failure);
    let message = failure.message;
    match kind {
        HostStoreFailureKind::EntryAlreadyExists => {
            PasswordEntryWriteError::already_exists(message)
        }
        HostStoreFailureKind::EntryNotFound => PasswordEntryWriteError::entry_not_found(message),
        HostStoreFailureKind::MissingPrivateKey => {
            PasswordEntryWriteError::MissingPrivateKey(message)
        }
        HostStoreFailureKind::LockedPrivateKey => {
            PasswordEntryWriteError::LockedPrivateKey(message)
        }
        HostStoreFailureKind::IncompatiblePrivateKey => {
            PasswordEntryWriteError::IncompatiblePrivateKey(message)
        }
        HostStoreFailureKind::Other => PasswordEntryWriteError::other(message),
    }
}

fn append_pass_entry_args<'a>(cmd: &mut Command, labels: impl IntoIterator<Item = &'a str>) {
    cmd.arg("--");
    cmd.args(labels);
}

fn configure_pass_show_command(cmd: &mut Command, label: &str) {
    cmd.arg("show");
    append_pass_entry_args(cmd, [label]);
}

fn configure_pass_insert_command(cmd: &mut Command, label: &str, overwrite: bool) {
    cmd.arg("insert").arg("-m");
    if overwrite {
        cmd.arg("-f");
    }
    append_pass_entry_args(cmd, [label]);
}

fn configure_pass_move_command(cmd: &mut Command, old_label: &str, new_label: &str) {
    cmd.arg("mv");
    append_pass_entry_args(cmd, [old_label, new_label]);
}

fn configure_pass_remove_command(cmd: &mut Command, label: &str) {
    cmd.arg("rm").arg("-f");
    append_pass_entry_args(cmd, [label]);
}

// `pass` may return success when an overwrite prompt is declined. Check before
// sending secret stdin so a new import cannot be reported as saved over another item.
fn reject_existing_destination(
    store_root: &str,
    label: &str,
) -> Result<(), PasswordEntryWriteError> {
    let path = keycord_stores::paths::desired_entry_file_path(store_root, label)
        .map_err(PasswordEntryWriteError::other)?;
    match std::fs::symlink_metadata(path) {
        Ok(_) => Err(PasswordEntryWriteError::already_exists(
            "That password entry already exists.",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(PasswordEntryWriteError::other(error.to_string())),
    }
}

fn validate_entry_label_for_read(label: &str) -> Result<(), PasswordEntryError> {
    validated_entry_label_path(label)
        .map(|_| ())
        .map_err(PasswordEntryError::other)
}

fn validate_entry_label_for_write(label: &str) -> Result<(), PasswordEntryWriteError> {
    validated_entry_label_path(label)
        .map(|_| ())
        .map_err(PasswordEntryWriteError::other)
}

pub struct HostEntryBackend<'a> {
    commands: &'a dyn HostEntryCommandPort,
}

impl<'a> HostEntryBackend<'a> {
    pub const fn new(commands: &'a dyn HostEntryCommandPort) -> Self {
        Self { commands }
    }

    pub fn read_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, PasswordEntryError> {
        self.read_password_entry_with_progress(store_root, label, &mut |_| {})
    }

    pub fn read_entry_bytes(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<Vec<u8>, PasswordEntryError> {
        self.read_entry_bytes_with_progress(store_root, label, &mut |_| {})
    }
    pub fn read_password_entry_with_progress(
        &self,
        store_root: &str,
        label: &str,
        progress: &mut dyn FnMut(PasswordEntryReadProgress),
    ) -> Result<String, PasswordEntryError> {
        let bytes = zeroize::Zeroizing::new(
            self.read_entry_bytes_with_progress(store_root, label, progress)?,
        );
        crate::file::entry_text(&bytes)
    }
    pub fn read_entry_bytes_with_progress(
        &self,
        store_root: &str,
        label: &str,
        report_progress: &mut dyn FnMut(PasswordEntryReadProgress),
    ) -> Result<Vec<u8>, PasswordEntryError> {
        validate_entry_label_for_read(label)?;
        let _ = report_progress;

        let mut configure = |cmd: &mut Command| configure_pass_show_command(cmd, label);
        let output = self
            .commands
            .run_store_command_output(
                store_root,
                "Read password entry",
                CommandLogOptions::SENSITIVE,
                &mut configure,
            )
            .map_err(PasswordEntryError::other)?;
        let output = ensure_host_command_success(HostStoreAction::ReadEntry, output, "pass failed")
            .map_err(password_entry_error_from_host_failure)?;
        Ok(output.stdout)
    }

    pub fn read_password_line(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<String, PasswordEntryError> {
        validate_entry_label_for_read(label)?;

        let mut configure = |cmd: &mut Command| configure_pass_show_command(cmd, label);
        let output = self
            .commands
            .run_store_command_output(
                store_root,
                "Read password entry for clipboard copy",
                CommandLogOptions::SENSITIVE,
                &mut configure,
            )
            .map_err(PasswordEntryError::other)?;
        let output = ensure_host_command_success(HostStoreAction::ReadLine, output, "pass failed")
            .map_err(password_entry_error_from_host_failure)?;
        crate::file::password_line(&crate::file::entry_text(&output.stdout)?)
    }

    pub const fn password_entry_is_readable(&self, _store_root: &str, _label: &str) -> bool {
        true
    }

    pub fn save_password_entry(
        &self,
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError> {
        self.save_entry_bytes(store_root, label, contents.as_bytes(), overwrite)
    }
    pub fn save_entry_bytes(
        &self,
        store_root: &str,
        label: &str,
        contents: &[u8],
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError> {
        validate_entry_label_for_write(label)?;
        if !overwrite {
            reject_existing_destination(store_root, label)?;
        }
        crate::file::validate_entry_bytes_path(contents, label)
            .map_err(PasswordEntryWriteError::other)?;

        let mut configure = |cmd: &mut Command| {
            configure_pass_insert_command(cmd, label, overwrite);
        };
        let output = self
            .commands
            .run_store_command_with_bytes(
                store_root,
                "Save password entry",
                contents,
                CommandLogOptions::SENSITIVE,
                &mut configure,
            )
            .map_err(PasswordEntryWriteError::other)?;

        ensure_host_command_success(HostStoreAction::SaveEntry, output, "pass insert failed")
            .map(|_| ())
            .map_err(password_entry_write_error_from_host_failure)
    }

    pub fn rename_password_entry(
        &self,
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), PasswordEntryWriteError> {
        validate_entry_label_for_write(old_label)?;
        validate_entry_label_for_write(new_label)?;
        if old_label != new_label {
            reject_existing_destination(store_root, new_label)?;
        }
        #[cfg(any(feature = "passkey", feature = "passless"))]
        if keycord_passkey::is_passkey_entry_label(old_label) {
            let contents = zeroize::Zeroizing::new(self.read_entry_bytes(store_root, old_label)?);
            crate::file::validate_entry_bytes_path(&contents, new_label)
                .map_err(PasswordEntryWriteError::other)?;
        }

        let mut configure = |cmd: &mut Command| {
            configure_pass_move_command(cmd, old_label, new_label);
        };
        let output = self
            .commands
            .run_store_command_output(
                store_root,
                "Rename password entry",
                CommandLogOptions::DEFAULT,
                &mut configure,
            )
            .map_err(PasswordEntryWriteError::other)?;

        ensure_host_command_success(HostStoreAction::RenameEntry, output, "pass mv failed")
            .map(|_| ())
            .map_err(password_entry_write_error_from_host_failure)
    }

    pub fn delete_password_entry(
        &self,
        store_root: &str,
        label: &str,
    ) -> Result<(), PasswordEntryWriteError> {
        validate_entry_label_for_write(label)?;

        let mut configure = |cmd: &mut Command| configure_pass_remove_command(cmd, label);
        let output = self
            .commands
            .run_store_command_output(
                store_root,
                "Delete password entry",
                CommandLogOptions::DEFAULT,
                &mut configure,
            )
            .map_err(PasswordEntryWriteError::other)?;

        ensure_host_command_success(HostStoreAction::DeleteEntry, output, "pass rm failed")
            .map(|_| ())
            .map_err(password_entry_write_error_from_host_failure)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        configure_pass_insert_command, configure_pass_move_command, configure_pass_remove_command,
        configure_pass_show_command, password_entry_error_from_host_failure,
        password_entry_write_error_from_host_failure, HostCommandFailure, HostEntryBackend,
        HostEntryCommandPort, HostStoreAction,
    };
    use crate::{PasswordEntryError, PasswordEntryWriteError};
    use keycord_runtime::CommandLogOptions;
    use std::process::{Command, ExitStatus, Output};

    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;

    #[cfg(windows)]
    use std::os::windows::process::ExitStatusExt;

    struct PanicCommandPort;

    impl HostEntryCommandPort for PanicCommandPort {
        fn run_store_command_output(
            &self,
            _store_root: &str,
            _action: &str,
            _log_options: CommandLogOptions,
            _configure: &mut dyn FnMut(&mut Command),
        ) -> Result<Output, String> {
            panic!("invalid labels must be rejected before command execution")
        }

        fn run_store_command_with_input(
            &self,
            _store_root: &str,
            _action: &str,
            _input: &str,
            _log_options: CommandLogOptions,
            _configure: &mut dyn FnMut(&mut Command),
        ) -> Result<Output, String> {
            panic!("invalid labels must be rejected before command execution")
        }
    }

    fn failed_output(stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn failed_output_with_stdout(stdout: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(1 << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    fn command_args(cmd: &Command) -> Vec<String> {
        cmd.get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    struct FixtureCommandPort {
        mutations: std::sync::atomic::AtomicUsize,
    }

    impl HostEntryCommandPort for FixtureCommandPort {
        fn run_store_command_output(
            &self,
            _root: &str,
            _action: &str,
            _options: CommandLogOptions,
            configure: &mut dyn FnMut(&mut Command),
        ) -> Result<Output, String> {
            let mut command = Command::new("pass");
            configure(&mut command);
            let args = command_args(&command);
            let stdout = if args.first().is_some_and(|arg| arg == "show") {
                include_bytes!("../../keycord-passkey/tests/fixtures/es256.b64").to_vec()
            } else {
                self.mutations
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Vec::new()
            };
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout,
                stderr: Vec::new(),
            })
        }

        fn run_store_command_with_input(
            &self,
            _root: &str,
            _action: &str,
            _input: &str,
            _options: CommandLogOptions,
            _configure: &mut dyn FnMut(&mut Command),
        ) -> Result<Output, String> {
            self.mutations
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Output {
                status: ExitStatus::from_raw(0),
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        }
    }

    #[test]
    fn host_password_reads_respect_the_passkey_feature() {
        let commands = FixtureCommandPort {
            mutations: std::sync::atomic::AtomicUsize::new(0),
        };
        let backend = HostEntryBackend::new(&commands);
        let result = backend.read_password_line("/unused", "example.com/test");
        if cfg!(feature = "passkey") {
            assert!(result.is_err());
        } else {
            assert_eq!(
                result.unwrap(),
                include_str!("../../keycord-passkey/tests/fixtures/es256.b64").trim()
            );
        }
    }

    #[cfg(feature = "passkey")]
    #[test]
    fn invalid_passkey_paths_never_reach_host_mutation_commands() {
        let commands = FixtureCommandPort {
            mutations: std::sync::atomic::AtomicUsize::new(0),
        };
        let backend = HostEntryBackend::new(&commands);
        let encoded = include_str!("../../keycord-passkey/tests/fixtures/es256.b64").trim();
        let credential = keycord_passkey::decode_passkey_storage_value(encoded).unwrap();
        let label = keycord_passkey::build_passkey_storage_entry(&credential)
            .unwrap()
            .label;
        assert!(backend
            .save_password_entry("/unused", "example.com/wrong-id", encoded, true)
            .is_err());
        assert!(backend
            .rename_password_entry("/unused", &label, "example.com/wrong-id")
            .is_err());
        assert_eq!(
            commands.mutations.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        backend
            .rename_password_entry("/unused", &label, &format!("moved/{label}"))
            .unwrap();
        assert_eq!(
            commands.mutations.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
    }

    #[test]
    fn pass_commands_terminate_option_parsing_for_entry_labels() {
        let mut show = Command::new("pass");
        configure_pass_show_command(&mut show, "-danger");
        assert_eq!(command_args(&show), vec!["show", "--", "-danger"]);

        let mut insert = Command::new("pass");
        configure_pass_insert_command(&mut insert, "-danger", true);
        assert_eq!(
            command_args(&insert),
            vec!["insert", "-m", "-f", "--", "-danger"]
        );

        let mut mv = Command::new("pass");
        configure_pass_move_command(&mut mv, "-old", "-new");
        assert_eq!(command_args(&mv), vec!["mv", "--", "-old", "-new"]);

        let mut rm = Command::new("pass");
        configure_pass_remove_command(&mut rm, "-danger");
        assert_eq!(command_args(&rm), vec!["rm", "-f", "--", "-danger"]);
    }

    #[test]
    fn deletes_entries_non_recursively() {
        let mut rm = Command::new("pass");
        configure_pass_remove_command(&mut rm, "team/entry");
        assert_eq!(command_args(&rm), vec!["rm", "-f", "--", "team/entry"]);
    }

    #[test]
    fn rejects_traversal_labels_before_command_execution() {
        let backend = HostEntryBackend::new(&PanicCommandPort);
        assert!(matches!(
            backend.read_password_entry("/tmp/unused", "team/../../escape"),
            Err(PasswordEntryError::Other(message)) if message == "Invalid password entry path."
        ));

        for result in [
            backend.save_password_entry("/tmp/unused", "team/../../escape", "secret", true),
            backend.rename_password_entry("/tmp/unused", "team/../../escape", "renamed"),
            backend.delete_password_entry("/tmp/unused", "team/../../escape"),
        ] {
            assert!(matches!(
                result,
                Err(PasswordEntryWriteError::Other(message))
                    if message == "Invalid password entry path."
            ));
        }
    }

    #[test]
    fn host_write_errors_classify_existing_and_missing_entries() {
        assert!(matches!(
            password_entry_write_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::SaveEntry,
                failed_output("That password entry already exists."),
                "pass insert failed",
            )),
            PasswordEntryWriteError::EntryAlreadyExists(_)
        ));
        assert!(matches!(
            password_entry_write_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::RenameEntry,
                failed_output("Password entry 'team/demo' was not found."),
                "pass mv failed",
            )),
            PasswordEntryWriteError::EntryNotFound(_)
        ));
    }

    #[test]
    fn host_read_errors_classify_pkesks_failures_as_incompatible_private_keys() {
        assert!(matches!(
            password_entry_error_from_host_failure(HostCommandFailure::from_output(
                HostStoreAction::ReadEntry,
                failed_output("no pkesks managed to decrypt the ciphertext"),
                "pass failed",
            )),
            PasswordEntryError::IncompatiblePrivateKey(_)
        ));
    }

    #[test]
    fn sensitive_read_failures_do_not_surface_stdout_contents() {
        let error = password_entry_error_from_host_failure(HostCommandFailure::from_output(
            HostStoreAction::ReadEntry,
            failed_output_with_stdout("supersecret\nusername: alice"),
            "pass failed",
        ));

        match error {
            PasswordEntryError::Other(message) => {
                assert!(message.contains("pass failed"));
                assert!(!message.contains("supersecret"));
                assert!(!message.contains("username: alice"));
            }
            _ => panic!("unexpected host read error variant"),
        }
    }
}

#[cfg(all(test, feature = "passless", unix))]
mod native_tests {
    use super::*;
    use std::{os::unix::process::ExitStatusExt, sync::Mutex};
    struct BinaryHost(Mutex<Vec<u8>>);
    impl HostEntryCommandPort for BinaryHost {
        fn run_store_command_output(
            &self,
            _: &str,
            _: &str,
            _: CommandLogOptions,
            _: &mut dyn FnMut(&mut Command),
        ) -> Result<Output, String> {
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: self.0.lock().unwrap().clone(),
                stderr: vec![],
            })
        }
        fn run_store_command_with_input(
            &self,
            _: &str,
            _: &str,
            _: &str,
            _: CommandLogOptions,
            _: &mut dyn FnMut(&mut Command),
        ) -> Result<Output, String> {
            panic!("Binary entry must not use a text port")
        }
        fn run_store_command_with_bytes(
            &self,
            _: &str,
            _: &str,
            input: &[u8],
            _: CommandLogOptions,
            _: &mut dyn FnMut(&mut Command),
        ) -> Result<Output, String> {
            *self.0.lock().unwrap() = input.to_vec();
            Ok(Output {
                status: std::process::ExitStatus::from_raw(0),
                stdout: vec![],
                stderr: vec![],
            })
        }
    }
    #[test]
    fn native_records_use_binary_host_io_and_never_reach_password_actions() {
        let original =
            include_bytes!("../../keycord-passkey/tests/fixtures/passless-ed25519-19.cbor");
        let host = BinaryHost(Mutex::new(original.to_vec()));
        let backend = HostEntryBackend::new(&host);
        let label = format!("fido2/example.com/{}", "42".repeat(32));
        assert_eq!(
            backend.read_entry_bytes("unused", &label).unwrap(),
            original
        );
        assert!(backend.read_password_entry("unused", &label).is_err());
        assert!(backend.read_password_line("unused", &label).is_err());
        assert!(backend
            .rename_password_entry("unused", &label, "example.com/wrong")
            .is_err());
        backend
            .save_entry_bytes("unused", &label, original, false)
            .unwrap();
        assert_eq!(&*host.0.lock().unwrap(), original);
        let mut broken = original.to_vec();
        broken.push(0);
        *host.0.lock().unwrap() = broken;
        assert!(backend.read_password_line("unused", &label).is_err());
    }
}
