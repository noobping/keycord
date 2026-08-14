use crate::command::{
    git_command_error, git_output_text, run_store_git_command, run_store_git_command_with_input,
    run_store_git_work_tree_command,
};
use crate::has_git_repository;
use keycord_runtime::capabilities::supports_host_command_features;
use keycord_runtime::{log_error, log_info, CommandLogOptions};
use sequoia_openpgp::policy::StandardPolicy;
use sequoia_openpgp::serialize::stream::{Armorer, Message, Signer};
use sequoia_openpgp::{self as openpgp, Cert};
use std::io::Cursor;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitPrivateKey {
    pub fingerprint: String,
    pub user_ids: Vec<String>,
}

#[derive(Clone, Copy)]
pub struct IntegratedGitPorts {
    pub list_private_keys: fn() -> Result<Vec<GitPrivateKey>, String>,
    pub private_key_requires_session_unlock: fn(&str) -> Result<bool, String>,
    pub unlocked_signing_cert: fn(&str) -> Result<Option<Arc<Cert>>, String>,
    pub sign_with_unlocked_hardware: fn(&str, &str) -> Result<Option<String>, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommitIdentity {
    name: String,
    email: String,
    signing_fingerprint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommitIdentitySource {
    ExplicitPrivateKey(String),
    MissingExplicitPrivateKey(String),
    MissingExplicitFingerprint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommitIdentityResolution {
    identity: CommitIdentity,
    source: CommitIdentitySource,
}

fn stage_git_paths(store_root: &str, paths: &[String]) -> Result<(), String> {
    let output = run_store_git_work_tree_command(
        store_root,
        "Stage password store Git changes",
        |cmd| {
            cmd.arg("add").arg("-A").arg("--").args(paths);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git add", &output))
    }
}

fn staged_git_paths_have_changes(store_root: &str, paths: &[String]) -> Result<bool, String> {
    let output = run_store_git_work_tree_command(
        store_root,
        "Check staged password store Git changes",
        |cmd| {
            cmd.args(["diff", "--cached", "--quiet", "--exit-code", "--"])
                .args(paths);
        },
        CommandLogOptions {
            accepted_exit_codes: &[1],
            ..CommandLogOptions::DEFAULT
        },
    )?;
    match output.status.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        _ => Err(git_command_error("git diff --cached", &output)),
    }
}

fn required_git_output_text(output: &std::process::Output) -> Result<String, String> {
    let text = git_output_text(output)?;
    if text.is_empty() {
        Err("Git command did not return output.".to_string())
    } else {
        Ok(text)
    }
}

fn write_git_tree(store_root: &str) -> Result<String, String> {
    let output = run_store_git_command(
        store_root,
        "Write password store Git tree",
        |cmd| {
            cmd.arg("write-tree");
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        required_git_output_text(&output)
    } else {
        Err(git_command_error("git write-tree", &output))
    }
}

fn head_oid(store_root: &str) -> Result<Option<String>, String> {
    let output = run_store_git_command(
        store_root,
        "Read password store Git HEAD",
        |cmd| {
            cmd.args(["rev-parse", "-q", "--verify", "HEAD^{commit}"]);
        },
        CommandLogOptions {
            accepted_exit_codes: &[1],
            ..CommandLogOptions::DEFAULT
        },
    )?;
    match output.status.code() {
        Some(0) => required_git_output_text(&output).map(Some),
        Some(1) => {
            log_info(format!(
                "Password store Git HEAD is missing for {store_root}; creating an initial commit."
            ));
            Ok(None)
        }
        _ => Err(git_command_error(
            "git rev-parse -q --verify HEAD^{commit}",
            &output,
        )),
    }
}

fn git_ident(store_root: &str, role: &str, identity: &CommitIdentity) -> Result<String, String> {
    let env_prefix = role.to_ascii_uppercase();
    let output = run_store_git_command(
        store_root,
        &format!("Resolve Git {role} identity"),
        |cmd| {
            cmd.env(format!("GIT_{env_prefix}_NAME"), &identity.name)
                .env(format!("GIT_{env_prefix}_EMAIL"), &identity.email)
                .arg("var")
                .arg(format!("GIT_{env_prefix}_IDENT"));
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        required_git_output_text(&output)
    } else {
        Err(git_command_error("git var", &output))
    }
}

fn preferred_commit_private_key(
    explicit_fingerprint: Option<&str>,
    ports: IntegratedGitPorts,
) -> Result<Option<GitPrivateKey>, String> {
    let Some(explicit_fingerprint) = explicit_fingerprint else {
        return Ok(None);
    };
    let keys = (ports.list_private_keys)()?;
    Ok(preferred_commit_private_key_from_values(
        explicit_fingerprint,
        &keys,
    ))
}

fn preferred_commit_private_key_from_values(
    explicit: &str,
    keys: &[GitPrivateKey],
) -> Option<GitPrivateKey> {
    keys.iter()
        .find(|key| key.fingerprint.eq_ignore_ascii_case(explicit))
        .cloned()
}

fn commit_signing_fingerprint(
    explicit_fingerprint: &str,
    ports: IntegratedGitPorts,
) -> Result<Option<String>, String> {
    let Some(key) = preferred_commit_private_key(Some(explicit_fingerprint), ports)? else {
        return Ok(None);
    };
    Ok(Some(key.fingerprint))
}

fn commit_signing_key_requiring_unlock(
    store_root: &str,
    fingerprint: String,
    ports: IntegratedGitPorts,
) -> Result<Option<String>, String> {
    if !supports_host_command_features() {
        return Ok(None);
    }
    if !has_git_repository(store_root) {
        return Ok(None);
    }

    if (ports.private_key_requires_session_unlock)(&fingerprint)? {
        return Ok(Some(fingerprint));
    }

    Ok(None)
}

pub fn git_commit_private_key_requiring_unlock(
    store_root: &str,
    explicit_fingerprint: Option<&str>,
    ports: IntegratedGitPorts,
) -> Result<Option<String>, String> {
    if !supports_host_command_features() {
        return Ok(None);
    }
    let Some(explicit_fingerprint) = explicit_fingerprint else {
        return Ok(None);
    };
    let Some(fingerprint) = commit_signing_fingerprint(explicit_fingerprint, ports)? else {
        return Ok(None);
    };
    commit_signing_key_requiring_unlock(store_root, fingerprint, ports)
}

fn synthetic_commit_email(fingerprint: &str) -> String {
    format!("git+{}@keycord.invalid", fingerprint.to_ascii_lowercase())
}

fn parse_private_key_user_id(user_id: &str, fingerprint: &str) -> (String, String) {
    let trimmed = user_id.trim();
    if trimmed.is_empty() {
        return (fingerprint.to_string(), synthetic_commit_email(fingerprint));
    }

    let email = trimmed.rfind('<').map_or_else(
        || (trimmed.contains('@') && !trimmed.contains(' ')).then(|| trimmed.to_string()),
        |start| {
            trimmed[start + 1..]
                .find('>')
                .map(|offset| trimmed[start + 1..start + 1 + offset].trim().to_string())
                .filter(|value| !value.is_empty())
        },
    );

    let name = trimmed.rfind('<').map_or_else(
        || trimmed.to_string(),
        |start| trimmed[..start].trim().trim_matches('"').to_string(),
    );

    let email = email.unwrap_or_else(|| synthetic_commit_email(fingerprint));
    let name = if name.is_empty() { email.clone() } else { name };

    (name, email)
}

fn commit_identity(
    explicit_fingerprint: Option<&str>,
    ports: IntegratedGitPorts,
) -> Result<CommitIdentityResolution, String> {
    if let Some(explicit_fingerprint) = explicit_fingerprint {
        if let Some(key) = preferred_commit_private_key(Some(explicit_fingerprint), ports)? {
            return Ok(CommitIdentityResolution {
                identity: commit_identity_from_private_key(&key),
                source: CommitIdentitySource::ExplicitPrivateKey(explicit_fingerprint.to_string()),
            });
        }

        return Ok(CommitIdentityResolution {
            identity: generic_commit_identity(),
            source: CommitIdentitySource::MissingExplicitPrivateKey(
                explicit_fingerprint.to_string(),
            ),
        });
    }

    Ok(CommitIdentityResolution {
        identity: generic_commit_identity(),
        source: CommitIdentitySource::MissingExplicitFingerprint,
    })
}

fn generic_commit_identity() -> CommitIdentity {
    CommitIdentity {
        name: "Keycord".to_string(),
        email: "git@keycord.invalid".to_string(),
        signing_fingerprint: None,
    }
}

fn commit_identity_from_private_key(key: &GitPrivateKey) -> CommitIdentity {
    let (name, email) = key
        .user_ids
        .iter()
        .find(|user_id| !user_id.trim().is_empty())
        .map_or_else(
            || {
                (
                    key.fingerprint.clone(),
                    synthetic_commit_email(&key.fingerprint),
                )
            },
            |user_id| parse_private_key_user_id(user_id, &key.fingerprint),
        );

    CommitIdentity {
        name,
        email,
        signing_fingerprint: Some(key.fingerprint.clone()),
    }
}

fn sign_commit_buffer(commit_buffer: &str, cert: &Cert) -> Result<String, String> {
    let policy = StandardPolicy::new();
    let signing_key = cert
        .keys()
        .with_policy(&policy, None)
        .alive()
        .revoked(false)
        .for_signing()
        .secret()
        .next()
        .ok_or_else(|| "That private key cannot sign Git commits.".to_string())?;
    let keypair = signing_key
        .key()
        .clone()
        .into_keypair()
        .map_err(|err| err.to_string())?;

    let mut sink = Vec::new();
    let message = Message::new(&mut sink);
    let message = Armorer::new(message)
        .kind(openpgp::armor::Kind::Signature)
        .build()
        .map_err(|err| err.to_string())?;
    let mut signer = Signer::new(message, keypair)
        .map_err(|err| err.to_string())?
        .detached()
        .build()
        .map_err(|err| err.to_string())?;
    std::io::copy(&mut Cursor::new(commit_buffer.as_bytes()), &mut signer)
        .map_err(|err| err.to_string())?;
    signer.finalize().map_err(|err| err.to_string())?;

    String::from_utf8(sink).map_err(|err| err.to_string())
}

fn build_commit_headers(
    tree_oid: &str,
    parent_oid: Option<&str>,
    author_ident: &str,
    committer_ident: &str,
) -> String {
    let mut headers = String::new();
    headers.push_str("tree ");
    headers.push_str(tree_oid);
    headers.push('\n');
    if let Some(parent_oid) = parent_oid {
        headers.push_str("parent ");
        headers.push_str(parent_oid);
        headers.push('\n');
    }
    headers.push_str("author ");
    headers.push_str(author_ident);
    headers.push('\n');
    headers.push_str("committer ");
    headers.push_str(committer_ident);
    headers.push('\n');
    headers
}

fn build_signed_commit_buffer(headers: &str, message: &str, signature: Option<&str>) -> String {
    let mut buffer = String::new();
    buffer.push_str(headers);

    if let Some(signature) = signature {
        let mut lines = signature.trim_end_matches('\n').lines();
        if let Some(first) = lines.next() {
            buffer.push_str("gpgsig ");
            buffer.push_str(first);
            buffer.push('\n');
            for line in lines {
                buffer.push(' ');
                buffer.push_str(line);
                buffer.push('\n');
            }
        }
    }

    buffer.push('\n');
    buffer.push_str(message);
    if !message.ends_with('\n') {
        buffer.push('\n');
    }
    buffer
}

fn write_commit_object(store_root: &str, commit_buffer: &str) -> Result<String, String> {
    let output = run_store_git_command_with_input(
        store_root,
        "Write password store Git commit object",
        commit_buffer,
        |cmd| {
            cmd.args(["hash-object", "-t", "commit", "-w", "--stdin"]);
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        required_git_output_text(&output)
    } else {
        Err(git_command_error("git hash-object", &output))
    }
}

fn update_git_head(
    store_root: &str,
    new_oid: &str,
    previous_oid: Option<&str>,
) -> Result<(), String> {
    let output = run_store_git_command(
        store_root,
        "Update password store Git HEAD",
        |cmd| {
            cmd.arg("update-ref").arg("HEAD").arg(new_oid);
            if let Some(previous_oid) = previous_oid {
                cmd.arg(previous_oid);
            }
        },
        CommandLogOptions::DEFAULT,
    )?;
    if output.status.success() {
        Ok(())
    } else {
        Err(git_command_error("git update-ref", &output))
    }
}

fn log_commit_identity_resolution(store_root: &str, resolution: &CommitIdentityResolution) {
    match &resolution.source {
        CommitIdentitySource::ExplicitPrivateKey(fingerprint) => log_info(format!(
            "Preparing password store Git commit for {store_root} with {name} <{email}> ({fingerprint}).",
            name = resolution.identity.name,
            email = resolution.identity.email,
        )),
        CommitIdentitySource::MissingExplicitPrivateKey(fingerprint) => log_info(format!(
            "Preparing password store Git commit for {store_root} without signing because private key {fingerprint} is not available in the app."
        )),
        CommitIdentitySource::MissingExplicitFingerprint => log_info(format!(
            "Preparing password store Git commit for {store_root} without signing because no recipients-derived private key was resolved."
        )),
    }
}

fn signature_for_commit(
    store_root: &str,
    resolution: &CommitIdentityResolution,
    unsigned_commit: &str,
    ports: IntegratedGitPorts,
) -> Result<Option<String>, String> {
    let Some(fingerprint) = resolution.identity.signing_fingerprint.as_deref() else {
        return Ok(None);
    };

    let Some(cert) = (ports.unlocked_signing_cert)(fingerprint)? else {
        let Some(signature) = (ports.sign_with_unlocked_hardware)(fingerprint, unsigned_commit)?
        else {
            log_info(format!(
                "Password store Git commit for {store_root} is unsigned because private key {fingerprint} is not unlocked in this session."
            ));
            return Ok(None);
        };

        log_info(format!(
            "Signing password store Git commit for {store_root} with {name} <{email}> ({fingerprint}).",
            name = resolution.identity.name,
            email = resolution.identity.email,
        ));
        log_info(format!(
            "Signed password store Git commit for {store_root} with private key {fingerprint}."
        ));
        return Ok(Some(signature));
    };

    log_info(format!(
        "Signing password store Git commit for {store_root} with {name} <{email}> ({fingerprint}).",
        name = resolution.identity.name,
        email = resolution.identity.email,
    ));
    let signature = sign_commit_buffer(unsigned_commit, &cert)?;
    log_info(format!(
        "Signed password store Git commit for {store_root} with private key {fingerprint}."
    ));
    Ok(Some(signature))
}

fn commit_git_paths(
    store_root: &str,
    message: &str,
    paths: &[String],
    explicit_fingerprint: Option<&str>,
    ports: IntegratedGitPorts,
) -> Result<(), String> {
    if !supports_host_command_features() {
        log_info(format!(
            "Skip password store Git commit for {store_root}: Git commands are only available on Linux."
        ));
        return Ok(());
    }
    if paths.is_empty() {
        log_info(format!(
            "Skip password store Git commit for {store_root}: no paths were provided."
        ));
        return Ok(());
    }

    if !has_git_repository(store_root) {
        log_info(format!(
            "Skip password store Git commit for {store_root}: the store is not a Git repository."
        ));
        return Ok(());
    }

    log_info(format!(
        "Prepare local password store Git commit for {store_root} with {} path(s): {}.",
        paths.len(),
        paths.join(", "),
    ));
    stage_git_paths(store_root, paths)?;
    if !staged_git_paths_have_changes(store_root, paths)? {
        log_info(format!(
            "Skip password store Git commit for {store_root}: the staged paths have no changes."
        ));
        return Ok(());
    }

    let tree_oid = write_git_tree(store_root)?;
    let parent_oid = head_oid(store_root)?;
    let identity = commit_identity(explicit_fingerprint, ports)?;
    log_commit_identity_resolution(store_root, &identity);
    let author_ident = git_ident(store_root, "author", &identity.identity)?;
    let committer_ident = git_ident(store_root, "committer", &identity.identity)?;
    let headers = build_commit_headers(
        &tree_oid,
        parent_oid.as_deref(),
        &author_ident,
        &committer_ident,
    );
    let unsigned_commit = build_signed_commit_buffer(&headers, message, None);
    let (signature, signed) = match signature_for_commit(
        store_root,
        &identity,
        &unsigned_commit,
        ports,
    ) {
        Ok(signature) => {
            let signed = signature.is_some();
            (signature, signed)
        }
        Err(err) => {
            log_error(format!(
                "Git commit signing unavailable for {store_root}: {err}. Falling back to an unsigned commit."
            ));
            (None, false)
        }
    };
    let commit_buffer = build_signed_commit_buffer(&headers, message, signature.as_deref());

    let commit_oid = write_commit_object(store_root, &commit_buffer)?;
    update_git_head(store_root, &commit_oid, parent_oid.as_deref())?;
    log_info(format!(
        "Created {} password store Git commit {commit_oid} for {store_root}.",
        if signed { "signed" } else { "unsigned" }
    ));
    Ok(())
}

pub fn password_entry_git_path(store_root: &Path, entry_path: &Path) -> Result<String, String> {
    let relative = entry_path
        .strip_prefix(store_root)
        .map_err(|_| "Invalid password entry path.".to_string())?;
    Ok(relative.to_string_lossy().to_string())
}

pub fn maybe_commit_git_paths(
    store_root: &str,
    message: &str,
    paths: impl IntoIterator<Item = String>,
    explicit_fingerprint: Option<&str>,
    ports: IntegratedGitPorts,
) {
    let mut paths = paths.into_iter().collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    if let Err(err) = commit_git_paths(store_root, message, &paths, explicit_fingerprint, ports) {
        log_error(format!(
            "Integrated backend Git commit failed for {store_root}: {err}"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_commit_headers, build_signed_commit_buffer, commit_identity,
        commit_identity_from_private_key, generic_commit_identity, parse_private_key_user_id,
        preferred_commit_private_key_from_values, CommitIdentity, CommitIdentityResolution,
        CommitIdentitySource, GitPrivateKey, IntegratedGitPorts,
    };
    use sequoia_openpgp::Cert;
    use std::sync::Arc;

    fn no_private_keys() -> Result<Vec<GitPrivateKey>, String> {
        Ok(Vec::new())
    }

    fn no_unlock_required(_fingerprint: &str) -> Result<bool, String> {
        Ok(false)
    }

    fn no_unlocked_cert(_fingerprint: &str) -> Result<Option<Arc<Cert>>, String> {
        Ok(None)
    }

    fn no_hardware_signature(
        _fingerprint: &str,
        _contents: &str,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn test_ports() -> IntegratedGitPorts {
        IntegratedGitPorts {
            list_private_keys: no_private_keys,
            private_key_requires_session_unlock: no_unlock_required,
            unlocked_signing_cert: no_unlocked_cert,
            sign_with_unlocked_hardware: no_hardware_signature,
        }
    }

    #[test]
    fn private_key_user_ids_map_to_git_identity() {
        assert_eq!(
            parse_private_key_user_id("Alice Example <alice@example.com>", "ABC123"),
            ("Alice Example".to_string(), "alice@example.com".to_string(),)
        );
        assert_eq!(
            parse_private_key_user_id("alice@example.com", "ABC123"),
            (
                "alice@example.com".to_string(),
                "alice@example.com".to_string(),
            )
        );
        assert_eq!(
            parse_private_key_user_id("Alice Example", "ABC123"),
            (
                "Alice Example".to_string(),
                "git+abc123@keycord.invalid".to_string(),
            )
        );
    }

    #[test]
    fn signed_commit_buffer_inserts_indented_gpgsig_header() {
        let headers = build_commit_headers(
            "deadbeef",
            Some("cafebabe"),
            "Alice <alice@example.com> 1 +0000",
            "Alice <alice@example.com> 1 +0000",
        );
        let commit = build_signed_commit_buffer(
            &headers,
            "Test commit",
            Some("-----BEGIN PGP SIGNATURE-----\nline-two\n-----END PGP SIGNATURE-----\n"),
        );

        assert!(commit.contains(
            "gpgsig -----BEGIN PGP SIGNATURE-----\n line-two\n -----END PGP SIGNATURE-----\n\nTest commit\n"
        ));
    }

    #[test]
    fn commit_identity_uses_the_explicit_private_key() {
        let key_a = GitPrivateKey {
            fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            user_ids: vec!["Key A <a@example.com>".to_string()],
        };
        let explicit = key_a.fingerprint.clone();
        let selected =
            preferred_commit_private_key_from_values(&explicit, std::slice::from_ref(&key_a))
                .expect("resolve explicit key");

        assert_eq!(
            commit_identity_from_private_key(&selected),
            CommitIdentity {
                name: "Key A".to_string(),
                email: "a@example.com".to_string(),
                signing_fingerprint: Some(key_a.fingerprint),
            }
        );
    }

    #[test]
    fn commit_identity_has_no_fallback_private_key_when_the_explicit_key_is_missing() {
        let key_a = GitPrivateKey {
            fingerprint: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
            user_ids: vec!["Key A <a@example.com>".to_string()],
        };
        let key_b = GitPrivateKey {
            fingerprint: "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".to_string(),
            user_ids: vec!["Key B <b@example.com>".to_string()],
        };
        assert_eq!(
            preferred_commit_private_key_from_values(
                "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
                &[key_a, key_b]
            ),
            None
        );
    }

    #[test]
    fn commit_identity_is_generic_and_unsigned_without_an_explicit_key() {
        assert_eq!(
            commit_identity(None, test_ports()).expect("build generic identity"),
            CommitIdentityResolution {
                identity: generic_commit_identity(),
                source: CommitIdentitySource::MissingExplicitFingerprint,
            }
        );
    }

    #[test]
    fn commit_identity_is_generic_when_the_explicit_key_is_missing() {
        assert_eq!(
            commit_identity(
                Some("CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC"),
                test_ports(),
            )
            .expect("build generic identity for missing key"),
            CommitIdentityResolution {
                identity: generic_commit_identity(),
                source: CommitIdentitySource::MissingExplicitPrivateKey(
                    "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC".to_string(),
                ),
            }
        );
    }
}
