//! Command execution with optional diagnostic stream logging.

use crate::diagnostics::{log_error, log_info};
use crate::worker::spawn_worker_or_panic;
use std::ffi::OsStr;
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;

/// Returns the most useful human-readable failure detail from command output.
pub fn output_failure_message(output: &Output, fallback: &str) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        format!("{fallback}: {}", output.status)
    }
}

/// Controls redaction, explicit environment visibility, and accepted exit codes.
#[derive(Clone, Copy, Debug, Default)]
pub struct CommandLogOptions {
    pub redact_stdout: bool,
    pub redact_stderr: bool,
    pub redact_stdin: bool,
    pub accepted_exit_codes: &'static [i32],
    /// Exact environment keys whose values the caller has classified as safe to log.
    pub safe_environment_keys: &'static [&'static str],
    /// Environment-key prefixes whose values the caller has classified as safe to log.
    pub safe_environment_prefixes: &'static [&'static str],
}

impl CommandLogOptions {
    pub const DEFAULT: Self = Self {
        redact_stdout: false,
        redact_stderr: false,
        redact_stdin: false,
        accepted_exit_codes: &[],
        safe_environment_keys: &[],
        safe_environment_prefixes: &[],
    };

    pub const SENSITIVE: Self = Self {
        redact_stdout: true,
        redact_stderr: true,
        redact_stdin: true,
        accepted_exit_codes: &[],
        safe_environment_keys: &[],
        safe_environment_prefixes: &[],
    };
}

fn shell_quote(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    if text.is_empty() {
        return "''".to_string();
    }
    if text.chars().all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-' | ':' | '=')
    }) {
        return text.into_owned();
    }

    let escaped = text.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

fn environment_is_safe_to_log(key: &OsStr, options: CommandLogOptions) -> bool {
    options
        .safe_environment_keys
        .iter()
        .any(|allowed| key == OsStr::new(allowed))
        || options.safe_environment_prefixes.iter().any(|prefix| {
            key.to_str()
                .is_some_and(|key_text| key_text.starts_with(prefix))
        })
}

fn describe_command(command: &Command, options: CommandLogOptions) -> String {
    let mut parts = Vec::new();
    for (key, value) in command.get_envs() {
        if !environment_is_safe_to_log(key, options) {
            continue;
        }
        if let Some(value) = value {
            let key_text = key.to_string_lossy();
            parts.push(format!("{key_text}={}", shell_quote(value)));
        }
    }
    parts.push(shell_quote(command.get_program()));
    for argument in command.get_args() {
        parts.push(shell_quote(argument));
    }
    parts.join(" ")
}

fn log_command_state(
    context: &str,
    command: &str,
    status: &str,
    stdin_was_provided: bool,
    redact_stdin: bool,
    is_error: bool,
) {
    let mut message = format!("{context}\n$ {command}\nstatus: {status}");
    if stdin_was_provided {
        message.push('\n');
        if redact_stdin {
            message.push_str("stdin: [redacted]");
        } else {
            message.push_str("stdin: provided");
        }
    }

    if is_error {
        log_error(message);
    } else {
        log_info(message);
    }
}

fn format_exit_status(status: ExitStatus) -> String {
    #[cfg(unix)]
    if let Some(signal) = status.signal() {
        return format!("signal {signal}");
    }

    status.to_string()
}

fn exit_status_is_error(status: ExitStatus, options: CommandLogOptions) -> bool {
    #[cfg(unix)]
    if status.signal().is_some() {
        return true;
    }

    match status.code() {
        Some(0) => false,
        Some(code) => !options.accepted_exit_codes.contains(&code),
        None => true,
    }
}

fn log_command_stream(context: &str, command: &str, label: &str, bytes: &[u8], redacted: bool) {
    if bytes.is_empty() {
        return;
    }

    let mut message = format!("{context}\n$ {command}\n{label}:");
    if redacted {
        message.push_str(" [redacted]");
        log_info(message);
        return;
    }

    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_end_matches(['\n', '\r']);
    if text.is_empty() {
        return;
    }

    message.push('\n');
    message.push_str(text);
    log_info(message);
}

fn spawn_stream_logger<R>(
    mut reader: R,
    context: String,
    command: String,
    label: &'static str,
    redacted: bool,
) -> thread::JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    spawn_worker_or_panic("command-stream-logger", move || {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut logged_redaction = false;

        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return Ok(bytes),
                Ok(read) => {
                    let chunk = &buffer[..read];
                    bytes.extend_from_slice(chunk);
                    if redacted {
                        if !logged_redaction {
                            log_command_stream(&context, &command, label, chunk, true);
                            logged_redaction = true;
                        }
                    } else {
                        log_command_stream(&context, &command, label, chunk, false);
                    }
                }
                Err(error) => {
                    log_error(format!(
                        "{context}\n$ {command}\nfailed to read {label}: {error}"
                    ));
                    return Err(error);
                }
            }
        }
    })
}

fn join_stream_logger(
    handle: Option<thread::JoinHandle<io::Result<Vec<u8>>>>,
    context: &str,
    command: &str,
    label: &str,
) -> io::Result<Vec<u8>> {
    let Some(handle) = handle else {
        return Ok(Vec::new());
    };

    handle.join().unwrap_or_else(|_| {
        let error = io::Error::other(format!("stream logger panicked while reading {label}"));
        log_error(format!("{context}\n$ {command}\n{error}"));
        Err(error)
    })
}

fn run_command_output_inner(
    command: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<Output> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let command_text = describe_command(command, options);

    match command.spawn() {
        Ok(mut child) => {
            log_command_state(context, &command_text, "running", false, false, false);

            let stdout_handle = child.stdout.take().map(|stdout| {
                spawn_stream_logger(
                    stdout,
                    context.to_string(),
                    command_text.clone(),
                    "stdout",
                    options.redact_stdout,
                )
            });
            let stderr_handle = child.stderr.take().map(|stderr| {
                spawn_stream_logger(
                    stderr,
                    context.to_string(),
                    command_text.clone(),
                    "stderr",
                    options.redact_stderr,
                )
            });

            let status = match child.wait() {
                Ok(status) => status,
                Err(error) => {
                    log_error(format!(
                        "{context}\n$ {command_text}\nfailed to wait: {error}"
                    ));
                    return Err(error);
                }
            };

            let stdout = join_stream_logger(stdout_handle, context, &command_text, "stdout")?;
            let stderr = join_stream_logger(stderr_handle, context, &command_text, "stderr")?;
            let output = Output {
                status,
                stdout,
                stderr,
            };

            log_command_state(
                context,
                &command_text,
                &format_exit_status(output.status),
                false,
                options.redact_stdin,
                exit_status_is_error(output.status, options),
            );
            Ok(output)
        }
        Err(error) => {
            log_error(format!(
                "{context}\n$ {command_text}\nfailed to start: {error}"
            ));
            Err(error)
        }
    }
}

fn spawn_input_writer(
    mut stdin: impl Write + Send + 'static,
    input: String,
) -> thread::JoinHandle<Result<(), String>> {
    spawn_worker_or_panic("command-stdin-writer", move || {
        stdin
            .write_all(input.as_bytes())
            .map_err(|error| format!("Failed to write command input: {error}"))
    })
}

fn join_input_writer(handle: thread::JoinHandle<Result<(), String>>) -> Result<(), String> {
    handle
        .join()
        .unwrap_or_else(|_| Err("Command input writer panicked.".to_string()))
}

/// Runs a command and collects both output streams.
pub fn run_command_output(
    command: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<Output> {
    run_command_output_inner(command, context, options)
}

/// Runs a command and returns its exit status after draining both output streams.
pub fn run_command_status(
    command: &mut Command,
    context: &str,
    options: CommandLogOptions,
) -> io::Result<ExitStatus> {
    run_command_output(command, context, options).map(|output| output.status)
}

/// Runs a command while writing the supplied UTF-8 input concurrently.
pub fn run_command_with_input(
    command: &mut Command,
    context: &str,
    input: &str,
    options: CommandLogOptions,
) -> Result<Output, String> {
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    let command_text = describe_command(command, options);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            log_error(format!(
                "{context}\n$ {command_text}\nfailed to start: {error}"
            ));
            return Err(format!("Failed to run command: {error}"));
        }
    };

    log_command_state(
        context,
        &command_text,
        "running",
        true,
        options.redact_stdin,
        false,
    );

    let stdout_handle = child.stdout.take().map(|stdout| {
        spawn_stream_logger(
            stdout,
            context.to_string(),
            command_text.clone(),
            "stdout",
            options.redact_stdout,
        )
    });
    let stderr_handle = child.stderr.take().map(|stderr| {
        spawn_stream_logger(
            stderr,
            context.to_string(),
            command_text.clone(),
            "stderr",
            options.redact_stderr,
        )
    });

    let Some(stdin) = child.stdin.take() else {
        log_error(format!("{context}\n$ {command_text}\nfailed to open stdin"));
        return Err("Failed to open stdin for command".to_string());
    };
    let input_writer = spawn_input_writer(stdin, input.to_string());

    let status = match child.wait() {
        Ok(status) => status,
        Err(error) => {
            log_error(format!(
                "{context}\n$ {command_text}\nfailed to wait: {error}"
            ));
            return Err(format!("Failed to wait for command: {error}"));
        }
    };

    let stdout = join_stream_logger(stdout_handle, context, &command_text, "stdout")
        .map_err(|error| format!("Failed to read command stdout: {error}"))?;
    let stderr = join_stream_logger(stderr_handle, context, &command_text, "stderr")
        .map_err(|error| format!("Failed to read command stderr: {error}"))?;
    let output = Output {
        status,
        stdout,
        stderr,
    };

    if let Err(error) = join_input_writer(input_writer) {
        log_error(format!(
            "{context}\n$ {command_text}\nfailed to write stdin: {error}"
        ));
        return Err(error);
    }

    log_command_state(
        context,
        &command_text,
        &format_exit_status(output.status),
        true,
        options.redact_stdin,
        exit_status_is_error(output.status, options),
    );

    Ok(output)
}

#[cfg(all(test, unix))]
mod tests {
    use super::{
        describe_command, output_failure_message, run_command_output, run_command_with_input,
        CommandLogOptions,
    };
    use crate::diagnostics::log_snapshot;
    use std::process::Command;

    #[test]
    fn command_output_collects_both_streams() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf 'stdout'; printf 'stderr' >&2; exit 3"]);

        let output = run_command_output(&mut command, "command output", CommandLogOptions::DEFAULT)
            .expect("command should run");

        assert_eq!(output.status.code(), Some(3));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "stdout");
        assert_eq!(String::from_utf8_lossy(&output.stderr), "stderr");
    }

    #[test]
    fn command_environment_is_hidden_unless_the_caller_marks_it_safe() {
        let mut command = Command::new("example-command");
        command
            .env("EXACT_VISIBLE", "/safe/exact")
            .env("VISIBLE_PREFIX_PATH", "/safe/prefix")
            .env("SECRET_TOKEN", "must-not-leak");

        let default_description = describe_command(&command, CommandLogOptions::DEFAULT);
        assert!(!default_description.contains("EXACT_VISIBLE"));
        assert!(!default_description.contains("VISIBLE_PREFIX_PATH"));
        assert!(!default_description.contains("must-not-leak"));

        let opted_in_description = describe_command(
            &command,
            CommandLogOptions {
                safe_environment_keys: &["EXACT_VISIBLE"],
                safe_environment_prefixes: &["VISIBLE_PREFIX_"],
                ..CommandLogOptions::DEFAULT
            },
        );
        assert!(opted_in_description.contains("EXACT_VISIBLE=/safe/exact"));
        assert!(opted_in_description.contains("VISIBLE_PREFIX_PATH=/safe/prefix"));
        assert!(!opted_in_description.contains("SECRET_TOKEN"));
        assert!(!opted_in_description.contains("must-not-leak"));
    }

    #[test]
    fn output_failure_prefers_stderr_over_stdout() {
        let output = Command::new("sh")
            .args([
                "-c",
                "printf 'stdout details'; printf 'stderr details' >&2; exit 9",
            ])
            .output()
            .expect("run shell");

        assert_eq!(
            output_failure_message(&output, "fallback"),
            "stderr details"
        );
    }

    #[test]
    fn command_input_is_written_without_deadlocking_on_large_output() {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "dd if=/dev/zero bs=65536 count=2 2>/dev/null; cat >/dev/null",
        ]);

        let output = run_command_with_input(
            &mut command,
            "large stdin",
            &"x".repeat(262_144),
            CommandLogOptions::SENSITIVE,
        )
        .expect("command should run");

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 131_072);
    }

    #[cfg(not(feature = "logging"))]
    #[test]
    fn disabled_logging_keeps_the_snapshot_empty() {
        assert_eq!(log_snapshot(), (0, 0, String::new()));
    }

    #[cfg(feature = "logging")]
    #[test]
    fn command_diagnostics_redact_credentials() {
        let marker = format!("url-redaction-test-{}", std::process::id());
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "printf 'https://user:secret@example.test/private/repo.git'",
        ]);

        run_command_output(&mut command, &marker, CommandLogOptions::DEFAULT)
            .expect("command should run");

        let (_, _, text) = log_snapshot();
        assert!(text.contains("https://redacted@example.test/private/repo.git"));
        assert!(!text.contains("user:secret@example.test"));
    }
}
