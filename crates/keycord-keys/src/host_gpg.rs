//! Host GPG key discovery and private-key synchronization operations.
//!
//! The Keys subject owns the exact GPG command contract, output
//! classification, and parsing. The composing application supplies only a
//! command runner so it can apply its host-access, sandbox, and logging policy.

use crate::sync::HostPrivateKeySyncPort;
#[cfg(feature = "audit")]
use keycord_runtime::log_error;
use keycord_runtime::CommandLogOptions;
#[cfg(feature = "audit")]
use sequoia_openpgp::{cert::CertParser, parse::Parse, Cert};
#[cfg(feature = "audit")]
use std::collections::HashSet;
use std::process::Output;

/// A host GPG private key as reported by `gpg --with-colons`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostGpgPrivateKeySummary {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
}

impl HostGpgPrivateKeySummary {
    #[must_use]
    pub fn title(&self) -> String {
        self.user_ids
            .first()
            .cloned()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "Unnamed host private key".to_string())
    }
}

/// One Keys-owned GPG invocation for the application to execute.
#[derive(Clone, Copy, Debug)]
pub struct HostGpgCommand<'a> {
    pub args: &'a [&'a str],
    pub input: Option<&'a str>,
    pub action: &'a str,
    pub log_options: CommandLogOptions,
}

/// Process output reduced to the fields needed for Keys-owned classification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostGpgCommandOutput {
    pub success: bool,
    pub status: String,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl From<Output> for HostGpgCommandOutput {
    fn from(output: Output) -> Self {
        Self {
            success: output.status.success(),
            status: output.status.to_string(),
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }
}

/// Configured host-command execution supplied by the composing application.
///
/// The port is deliberately object-safe: Keys chooses the exact arguments,
/// stdin, diagnostic label, and redaction policy; the application only runs
/// the command in its configured host environment.
pub trait HostGpgCommandPort {
    fn run_gpg(&self, command: HostGpgCommand<'_>) -> Result<HostGpgCommandOutput, String>;
}

#[derive(Clone, Copy)]
pub struct HostGpgBackend<'a> {
    commands: &'a dyn HostGpgCommandPort,
}

impl<'a> HostGpgBackend<'a> {
    #[must_use]
    pub const fn new(commands: &'a dyn HostGpgCommandPort) -> Self {
        Self { commands }
    }

    fn run(
        &self,
        args: &[&str],
        input: Option<&str>,
        action: &str,
        log_options: CommandLogOptions,
    ) -> Result<HostGpgCommandOutput, String> {
        self.commands.run_gpg(HostGpgCommand {
            args,
            input,
            action,
            log_options,
        })
    }

    pub fn list_private_keys(&self) -> Result<Vec<HostGpgPrivateKeySummary>, String> {
        let output = self.run(
            &[
                "--batch",
                "--with-colons",
                "--fingerprint",
                "--list-secret-keys",
            ],
            None,
            "Inspect host GPG private keys",
            CommandLogOptions::DEFAULT,
        )?;
        let output = ensure_success(output, "gpg --list-secret-keys failed")?;
        Ok(parse_host_gpg_private_keys(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    #[cfg(feature = "audit")]
    pub fn available_public_certs(&self) -> Result<Vec<Cert>, String> {
        let output = self.run(
            &["--batch", "--export"],
            None,
            "Export host GPG public keys",
            CommandLogOptions::DEFAULT,
        )?;
        let output = ensure_success(output, "gpg --export failed")?;
        parse_host_gpg_public_certs(&output.stdout)
    }

    pub fn armored_private_key(&self, fingerprint: &str) -> Result<String, String> {
        let output = self.run(
            &[
                "--batch",
                "--yes",
                "--armor",
                "--export-secret-keys",
                fingerprint,
            ],
            None,
            "Export host GPG private key",
            CommandLogOptions::SENSITIVE,
        )?;
        let output = ensure_success(output, "gpg --export-secret-keys failed")?;
        String::from_utf8(output.stdout).map_err(|err| err.to_string())
    }

    pub fn import_private_key_bytes(&self, bytes: &[u8]) -> Result<(), String> {
        let input = std::str::from_utf8(bytes).map_err(|err| err.to_string())?;
        let output = self.run(
            &["--batch", "--yes", "--import"],
            Some(input),
            "Import host GPG private key",
            CommandLogOptions::SENSITIVE,
        )?;
        ensure_success(output, "gpg --import failed").map(|_| ())
    }

    pub fn delete_private_key(&self, fingerprint: &str) -> Result<(), String> {
        let output = self.run(
            &["--batch", "--yes", "--delete-secret-keys", fingerprint],
            None,
            "Delete host GPG private key",
            CommandLogOptions::DEFAULT,
        )?;
        ensure_success(output, "gpg --delete-secret-keys failed").map(|_| ())
    }
}

impl HostPrivateKeySyncPort for HostGpgBackend<'_> {
    fn list_private_key_fingerprints(&self) -> Result<Vec<String>, String> {
        self.list_private_keys().map(|keys| {
            keys.into_iter()
                .map(|key| key.fingerprint)
                .collect::<Vec<_>>()
        })
    }

    fn export_private_key(&self, fingerprint: &str) -> Result<String, String> {
        self.armored_private_key(fingerprint)
    }

    fn import_private_key(&self, bytes: &[u8]) -> Result<(), String> {
        self.import_private_key_bytes(bytes)
    }

    fn delete_private_key(&self, fingerprint: &str) -> Result<(), String> {
        self.delete_private_key(fingerprint)
    }
}

fn ensure_success(
    output: HostGpgCommandOutput,
    fallback: &str,
) -> Result<HostGpgCommandOutput, String> {
    if output.success {
        return Ok(output);
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("{fallback}: {}", output.status))
    } else {
        Err(stderr)
    }
}

fn parse_host_gpg_private_keys(output: &str) -> Vec<HostGpgPrivateKeySummary> {
    #[derive(Default)]
    struct PartialHostKey {
        fingerprint: Option<String>,
        user_ids: Vec<String>,
        awaiting_primary_fpr: bool,
    }

    fn finish_key(
        partial: Option<PartialHostKey>,
        keys: &mut Vec<HostGpgPrivateKeySummary>,
    ) -> Option<PartialHostKey> {
        let partial = partial?;
        let fingerprint = partial
            .fingerprint
            .filter(|value| !value.trim().is_empty())?;
        if keys
            .iter()
            .any(|existing| existing.fingerprint.eq_ignore_ascii_case(&fingerprint))
        {
            return None;
        }

        keys.push(HostGpgPrivateKeySummary {
            fingerprint,
            user_ids: partial
                .user_ids
                .into_iter()
                .filter(|value| !value.trim().is_empty())
                .collect(),
        });
        None
    }

    fn colon_field(line: &str, index: usize) -> Option<&str> {
        line.split(':').nth(index).map(str::trim)
    }

    fn user_id_field(line: &str) -> &str {
        colon_field(line, 9)
            .filter(|value| !value.is_empty())
            .or_else(|| colon_field(line, 7).filter(|value| !value.is_empty()))
            .unwrap_or_default()
    }

    let mut keys = Vec::new();
    let mut current = None;

    for line in output.lines() {
        let mut fields = line.split(':');
        let Some(record_type) = fields.next() else {
            continue;
        };

        match record_type {
            "sec" => {
                let _ = finish_key(current.take(), &mut keys);
                current = Some(PartialHostKey {
                    fingerprint: None,
                    user_ids: Vec::new(),
                    awaiting_primary_fpr: true,
                });
            }
            "fpr" => {
                let Some(current) = current.as_mut() else {
                    continue;
                };
                if !current.awaiting_primary_fpr {
                    continue;
                }
                let fingerprint = colon_field(line, 9).unwrap_or_default().to_string();
                if fingerprint.is_empty() {
                    continue;
                }
                current.fingerprint = Some(fingerprint);
                current.awaiting_primary_fpr = false;
            }
            "uid" => {
                let Some(current) = current.as_mut() else {
                    continue;
                };
                let user_id = user_id_field(line).to_string();
                if !user_id.is_empty() {
                    current.user_ids.push(user_id);
                }
            }
            _ => {}
        }
    }

    finish_key(current, &mut keys);
    keys.sort_by(|left, right| {
        left.title()
            .to_ascii_lowercase()
            .cmp(&right.title().to_ascii_lowercase())
            .then_with(|| left.fingerprint.cmp(&right.fingerprint))
    });
    keys
}

#[cfg(feature = "audit")]
fn parse_host_gpg_public_certs(bytes: &[u8]) -> Result<Vec<Cert>, String> {
    let parser = CertParser::from_bytes(bytes).map_err(|err| err.to_string())?;
    let mut certs = Vec::new();
    let mut seen = HashSet::new();

    for cert in parser {
        match cert {
            Ok(cert) => {
                let cert = cert.strip_secret_key_material();
                let fingerprint = cert.fingerprint().to_hex();
                if seen.insert(fingerprint) {
                    certs.push(cert);
                }
            }
            Err(err) => {
                log_error(format!(
                    "Ignoring invalid host GPG public key while loading audit verification keys: {err}"
                ));
            }
        }
    }

    Ok(certs)
}

#[cfg(test)]
mod tests {
    use super::{
        parse_host_gpg_private_keys, HostGpgBackend, HostGpgCommand, HostGpgCommandOutput,
        HostGpgCommandPort, HostGpgPrivateKeySummary,
    };
    #[cfg(feature = "audit")]
    use sequoia_openpgp::{cert::CertBuilder, serialize::Serialize};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct RecordedCommand {
        args: Vec<String>,
        input: Option<String>,
        action: String,
        sensitive: bool,
    }

    #[derive(Default)]
    struct FakeHostGpgCommandPort {
        outputs: RefCell<VecDeque<Result<HostGpgCommandOutput, String>>>,
        commands: RefCell<Vec<RecordedCommand>>,
    }

    impl FakeHostGpgCommandPort {
        fn with_outputs(outputs: impl IntoIterator<Item = HostGpgCommandOutput>) -> Self {
            Self {
                outputs: RefCell::new(outputs.into_iter().map(Ok).collect()),
                commands: RefCell::new(Vec::new()),
            }
        }

        fn commands(&self) -> Vec<RecordedCommand> {
            self.commands.borrow().clone()
        }
    }

    impl HostGpgCommandPort for FakeHostGpgCommandPort {
        fn run_gpg(&self, command: HostGpgCommand<'_>) -> Result<HostGpgCommandOutput, String> {
            self.commands.borrow_mut().push(RecordedCommand {
                args: command.args.iter().map(|arg| (*arg).to_string()).collect(),
                input: command.input.map(str::to_string),
                action: command.action.to_string(),
                sensitive: command.log_options.redact_stdin
                    && command.log_options.redact_stdout
                    && command.log_options.redact_stderr,
            });
            self.outputs
                .borrow_mut()
                .pop_front()
                .expect("fake host GPG output")
        }
    }

    fn success(stdout: impl Into<Vec<u8>>) -> HostGpgCommandOutput {
        HostGpgCommandOutput {
            success: true,
            status: "exit status: 0".to_string(),
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn host_gpg_summary_uses_first_user_id_or_fallback_title() {
        let named = HostGpgPrivateKeySummary {
            fingerprint: "AA".to_string(),
            user_ids: vec!["Alice".to_string(), "Other".to_string()],
        };
        let unnamed = HostGpgPrivateKeySummary {
            fingerprint: "BB".to_string(),
            user_ids: Vec::new(),
        };

        assert_eq!(named.title(), "Alice");
        assert_eq!(unnamed.title(), "Unnamed host private key");
    }

    #[test]
    fn host_gpg_parser_keeps_primary_fingerprint_and_user_ids() {
        let parsed = parse_host_gpg_private_keys(
            "\
sec:u:255:22:PRIMARY:1:::::::scESC:::+:::23::0:\n\
fpr:::::::::AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:\n\
grp:::::::::group:\n\
uid:u::::1::Alice Example <alice@example.com>::::::::::0:\n\
uid:u::::1::Alice Work <alice@work.example>::::::::::0:\n\
ssb:u:255:18:SUB:1:::::::e:::+:::23:\n\
fpr:::::::::BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB:\n",
        );

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].fingerprint,
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
        );
        assert_eq!(
            parsed[0].user_ids,
            vec![
                "Alice Example <alice@example.com>".to_string(),
                "Alice Work <alice@work.example>".to_string()
            ]
        );
    }

    #[test]
    fn host_gpg_parser_ignores_duplicate_or_incomplete_blocks() {
        let parsed = parse_host_gpg_private_keys(
            "\
sec:u:::::::\n\
uid:u::::1::Missing Fingerprint:::::::\n\
sec:u:255:22:PRIMARY:1:::::::scESC:::+:::23::0:\n\
fpr:::::::::AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:\n\
uid:u::::1::Alice Example <alice@example.com>::::::::::0:\n\
sec:u:255:22:PRIMARY:1:::::::scESC:::+:::23::0:\n\
fpr:::::::::AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:\n\
uid:u::::1::Duplicate Alice <alice@example.com>::::::::::0:\n",
        );

        assert_eq!(parsed.len(), 1);
        assert_eq!(
            parsed[0].user_ids,
            vec!["Alice Example <alice@example.com>".to_string()]
        );
    }

    #[test]
    fn host_gpg_discovery_lists_imported_secret_keys_and_owns_exact_commands() {
        let port = FakeHostGpgCommandPort::with_outputs([
            success(Vec::new()),
            success(
                b"sec:u:255:22:PRIMARY:1:::::::scESC:::+:::23::0:\n\
fpr:::::::::AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA:\n\
uid:u::::1::Alice Example <alice@example.com>::::::::::0:\n"
                    .to_vec(),
            ),
        ]);
        let backend = HostGpgBackend::new(&port);

        backend
            .import_private_key_bytes(b"ARMORED")
            .expect("import host GPG private key");
        let fingerprints = crate::HostPrivateKeySyncPort::list_private_key_fingerprints(&backend)
            .expect("list host GPG fingerprints");

        assert_eq!(
            fingerprints,
            vec!["AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string()]
        );
        assert_eq!(
            port.commands(),
            vec![
                RecordedCommand {
                    args: ["--batch", "--yes", "--import"]
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    input: Some("ARMORED".to_string()),
                    action: "Import host GPG private key".to_string(),
                    sensitive: true,
                },
                RecordedCommand {
                    args: vec![
                        "--batch".to_string(),
                        "--with-colons".to_string(),
                        "--fingerprint".to_string(),
                        "--list-secret-keys".to_string(),
                    ],
                    input: None,
                    action: "Inspect host GPG private keys".to_string(),
                    sensitive: false,
                },
            ]
        );
    }

    #[test]
    fn host_gpg_sync_operations_preserve_args_input_and_redaction() {
        let port = FakeHostGpgCommandPort::with_outputs([
            success(b"-----BEGIN PGP PRIVATE KEY BLOCK-----\n".to_vec()),
            success(Vec::new()),
            success(Vec::new()),
        ]);
        let backend = HostGpgBackend::new(&port);

        assert_eq!(
            crate::HostPrivateKeySyncPort::export_private_key(&backend, "FINGERPRINT")
                .expect("export host private key"),
            "-----BEGIN PGP PRIVATE KEY BLOCK-----\n"
        );
        crate::HostPrivateKeySyncPort::import_private_key(&backend, b"ARMORED")
            .expect("import host private key");
        crate::HostPrivateKeySyncPort::delete_private_key(&backend, "FINGERPRINT")
            .expect("delete host private key");

        assert_eq!(
            port.commands(),
            vec![
                RecordedCommand {
                    args: [
                        "--batch",
                        "--yes",
                        "--armor",
                        "--export-secret-keys",
                        "FINGERPRINT",
                    ]
                    .into_iter()
                    .map(str::to_string)
                    .collect(),
                    input: None,
                    action: "Export host GPG private key".to_string(),
                    sensitive: true,
                },
                RecordedCommand {
                    args: ["--batch", "--yes", "--import"]
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    input: Some("ARMORED".to_string()),
                    action: "Import host GPG private key".to_string(),
                    sensitive: true,
                },
                RecordedCommand {
                    args: ["--batch", "--yes", "--delete-secret-keys", "FINGERPRINT"]
                        .into_iter()
                        .map(str::to_string)
                        .collect(),
                    input: None,
                    action: "Delete host GPG private key".to_string(),
                    sensitive: false,
                },
            ]
        );
    }

    #[test]
    fn host_gpg_failure_prefers_stderr_and_falls_back_to_status() {
        let stderr_port = FakeHostGpgCommandPort::with_outputs([HostGpgCommandOutput {
            success: false,
            status: "exit status: 2".to_string(),
            stdout: Vec::new(),
            stderr: b"permission denied\n".to_vec(),
        }]);
        let err = HostGpgBackend::new(&stderr_port)
            .delete_private_key("FINGERPRINT")
            .expect_err("delete should fail");
        assert_eq!(err, "permission denied");

        let status_port = FakeHostGpgCommandPort::with_outputs([HostGpgCommandOutput {
            success: false,
            status: "exit status: 2".to_string(),
            stdout: Vec::new(),
            stderr: Vec::new(),
        }]);
        let err = HostGpgBackend::new(&status_port)
            .delete_private_key("FINGERPRINT")
            .expect_err("delete should fail");
        assert_eq!(err, "gpg --delete-secret-keys failed: exit status: 2");
    }

    #[test]
    fn host_gpg_import_rejects_non_utf8_before_command_execution() {
        let port = FakeHostGpgCommandPort::default();
        let err = HostGpgBackend::new(&port)
            .import_private_key_bytes(&[0xff])
            .expect_err("non-UTF-8 import should fail");

        assert!(err.contains("utf-8"));
        assert!(port.commands().is_empty());
    }

    #[cfg(feature = "audit")]
    #[test]
    fn host_gpg_public_key_export_reads_multiple_public_certs() {
        let (alice, _) = CertBuilder::general_purpose(Some("alice@example.com"))
            .generate()
            .expect("generate alice cert");
        let (bob, _) = CertBuilder::general_purpose(Some("bob@example.com"))
            .generate()
            .expect("generate bob cert");
        let mut bytes = Vec::new();
        alice
            .as_tsk()
            .serialize(&mut bytes)
            .expect("serialize alice secret cert");
        bob.as_tsk()
            .serialize(&mut bytes)
            .expect("serialize bob secret cert");
        let port = FakeHostGpgCommandPort::with_outputs([success(bytes)]);

        let certs = HostGpgBackend::new(&port)
            .available_public_certs()
            .expect("parse host public certs");

        assert_eq!(certs.len(), 2);
        assert!(certs
            .iter()
            .all(|cert| !cert.is_tsk() && cert.keys().secret().next().is_none()));
        assert!(certs
            .iter()
            .any(|cert| cert.fingerprint() == alice.fingerprint()));
        assert!(certs
            .iter()
            .any(|cert| cert.fingerprint() == bob.fingerprint()));
        assert_eq!(
            port.commands(),
            vec![RecordedCommand {
                args: vec!["--batch".to_string(), "--export".to_string()],
                input: None,
                action: "Export host GPG public keys".to_string(),
                sensitive: false,
            }]
        );
    }
}
