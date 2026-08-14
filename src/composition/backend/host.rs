use super::integrated::try_initialize_empty_store_recipients;
#[cfg(target_os = "linux")]
use crate::composition::backend::command::{run_host_program_output, run_host_program_with_input};
use crate::composition::backend::{
    command::{run_store_command_output, run_store_command_with_input},
    PasswordEntryError, PasswordEntryWriteError, StoreRecipients, StoreRecipientsError,
    StoreRecipientsPrivateKeyRequirement,
};
use keycord_entries::host::{HostEntryBackend, HostEntryCommandPort};
use keycord_git::{ensure_store_git_repository, has_git_repository};
#[cfg(target_os = "linux")]
use keycord_keys::{HostGpgBackend, HostGpgCommand, HostGpgCommandOutput, HostGpgCommandPort};
use keycord_runtime::CommandLogOptions;
use keycord_stores::host::{self as store_core, configure_pass_init_command, HostStorePorts};
use std::process::{Command, Output};

struct RootHostStorePorts;

struct RootHostEntryCommandPort;

#[cfg(target_os = "linux")]
struct RootHostGpgCommandPort;

static ROOT_HOST_ENTRY_COMMAND_PORT: RootHostEntryCommandPort = RootHostEntryCommandPort;

#[cfg(target_os = "linux")]
static ROOT_HOST_GPG_COMMAND_PORT: RootHostGpgCommandPort = RootHostGpgCommandPort;

impl HostEntryCommandPort for RootHostEntryCommandPort {
    fn run_store_command_output(
        &self,
        store_root: &str,
        action: &str,
        log_options: CommandLogOptions,
        configure: &mut dyn FnMut(&mut Command),
    ) -> Result<Output, String> {
        run_store_command_output(store_root, action, log_options, |cmd| configure(cmd))
    }

    fn run_store_command_with_input(
        &self,
        store_root: &str,
        action: &str,
        input: &str,
        log_options: CommandLogOptions,
        configure: &mut dyn FnMut(&mut Command),
    ) -> Result<Output, String> {
        run_store_command_with_input(store_root, action, input, log_options, |cmd| configure(cmd))
    }
}

fn host_entry_backend() -> HostEntryBackend<'static> {
    HostEntryBackend::new(&ROOT_HOST_ENTRY_COMMAND_PORT)
}

#[cfg(target_os = "linux")]
impl HostGpgCommandPort for RootHostGpgCommandPort {
    fn run_gpg(&self, command: HostGpgCommand<'_>) -> Result<HostGpgCommandOutput, String> {
        let output = if let Some(input) = command.input {
            run_host_program_with_input(
                "gpg",
                command.args,
                input,
                command.action,
                command.log_options,
            )
        } else {
            run_host_program_output("gpg", command.args, command.action, command.log_options)
        }?;
        Ok(output.into())
    }
}

#[cfg(target_os = "linux")]
pub(crate) const fn host_gpg_backend() -> HostGpgBackend<'static> {
    HostGpgBackend::new(&ROOT_HOST_GPG_COMMAND_PORT)
}

impl HostStorePorts for RootHostStorePorts {
    fn try_initialize_empty_store_recipients(
        &self,
        store_root: &str,
        recipients: &StoreRecipients,
        private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    ) -> Result<bool, String> {
        try_initialize_empty_store_recipients(store_root, recipients, private_key_requirement)
    }

    fn run_pass_init(&self, store_root: &str, recipients: &[String]) -> Result<Output, String> {
        run_store_command_output(
            store_root,
            "Save password store recipients",
            CommandLogOptions::DEFAULT,
            |cmd| configure_pass_init_command(cmd, recipients),
        )
    }

    fn has_git_repository(&self, store_root: &str) -> bool {
        has_git_repository(store_root)
    }

    fn ensure_git_repository(&self, store_root: &str) -> Result<(), String> {
        ensure_store_git_repository(store_root)
    }
}

pub(super) fn read_password_entry(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    host_entry_backend().read_password_entry(store_root, label)
}

pub(super) fn read_password_entry_with_progress(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    host_entry_backend().read_password_entry(store_root, label)
}

pub(super) fn read_password_line(
    store_root: &str,
    label: &str,
) -> Result<String, PasswordEntryError> {
    host_entry_backend().read_password_line(store_root, label)
}

pub(super) fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    host_entry_backend().password_entry_is_readable(store_root, label)
}

pub(super) fn save_password_entry(
    store_root: &str,
    label: &str,
    contents: &str,
    overwrite: bool,
) -> Result<(), PasswordEntryWriteError> {
    host_entry_backend().save_password_entry(store_root, label, contents, overwrite)
}

pub(super) fn rename_password_entry(
    store_root: &str,
    old_label: &str,
    new_label: &str,
) -> Result<(), PasswordEntryWriteError> {
    host_entry_backend().rename_password_entry(store_root, old_label, new_label)
}

pub(super) fn delete_password_entry(
    store_root: &str,
    label: &str,
) -> Result<(), PasswordEntryWriteError> {
    host_entry_backend().delete_password_entry(store_root, label)
}

pub(super) fn save_store_recipients(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    save_store_recipients_with_progress(store_root, recipients, private_key_requirement)
}

pub(super) fn save_store_recipients_with_progress(
    store_root: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    store_core::save_store_recipients_with(
        &RootHostStorePorts,
        store_root,
        recipients,
        private_key_requirement,
    )
}

pub(super) fn save_store_recipients_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    store_core::save_store_recipients_for_relative_dir_with(
        &RootHostStorePorts,
        store_root,
        relative_dir,
        recipients,
        private_key_requirement,
    )
}

pub(super) fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    Ok(store_core::store_recipients_private_key_requiring_unlock(
        store_root,
    ))
}

pub(super) fn store_recipients_private_key_requiring_unlock_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
) -> Result<Option<String>, String> {
    store_core::store_recipients_private_key_requiring_unlock_for_relative_dir(
        store_root,
        relative_dir,
    )
}

#[cfg(all(target_os = "linux", feature = "audit"))]
pub(super) fn available_host_gpg_public_certs() -> Result<Vec<sequoia_openpgp::Cert>, String> {
    host_gpg_backend().available_public_certs()
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{save_password_entry, save_store_recipients};
    use crate::composition::backend::test_support::{
        assert_entry_is_encrypted_for_each_recipient, SystemBackendTestEnv,
    };
    use crate::composition::backend::{StoreRecipients, StoreRecipientsPrivateKeyRequirement};
    use keycord_git::has_git_repository;
    use keycord_preferences::Preferences;
    use sequoia_openpgp::serialize::SerializeInto;

    #[test]
    fn host_backend_encrypts_entries_for_all_store_recipients() {
        assert_entry_is_encrypted_for_each_recipient(
            |store_root, recipients| {
                save_store_recipients(
                    store_root,
                    &StoreRecipients::new(recipients.to_vec()),
                    StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
                )
                .map_err(|err| err.to_string())
            },
            |store_root, label, contents| {
                save_password_entry(store_root, label, contents, true)
                    .map_err(|err| err.to_string())
            },
        );
    }

    #[test]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "SystemBackendTestEnv must stay alive for the full test to keep the temp store and env vars in place."
    )]
    fn host_backend_initializes_new_stores_without_pass_init() {
        let env = SystemBackendTestEnv::new();

        let key = SystemBackendTestEnv::generate_secret_key("Recipient <host-create@example.com>")
            .expect("generate host recipient key");
        SystemBackendTestEnv::import_public_key(&key.public_key_bytes)
            .expect("import host recipient key");
        SystemBackendTestEnv::trust_public_key(&key.fingerprint_hex)
            .expect("trust host recipient key");
        Preferences::new()
            .set_command("keycord-pass-command-that-does-not-exist")
            .expect("set missing pass command");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(
            &store_root,
            &StoreRecipients::new(vec![key.fingerprint_hex.clone()]),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save store recipients");

        assert!(has_git_repository(&store_root));
        assert_eq!(
            std::fs::read_to_string(env.store_root().join(".gpg-id"))
                .expect("read initialized store recipients file"),
            format!("{}\n", key.fingerprint_hex)
        );
    }

    #[test]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "SystemBackendTestEnv must stay alive for the full test to keep the temp store and env vars in place."
    )]
    fn host_backend_saves_entries_with_empty_password_lines() {
        let env = SystemBackendTestEnv::new();

        let key = SystemBackendTestEnv::generate_secret_key("Recipient <host-empty@example.com>")
            .expect("generate host recipient key");
        let secret_key_bytes = key
            .cert
            .as_tsk()
            .armored()
            .to_vec()
            .expect("serialize armored secret key");
        super::host_gpg_backend()
            .import_private_key_bytes(&secret_key_bytes)
            .expect("import host secret key");
        SystemBackendTestEnv::import_public_key(&key.public_key_bytes)
            .expect("import host recipient key");
        SystemBackendTestEnv::trust_public_key(&key.fingerprint_hex)
            .expect("trust host recipient key");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(
            &store_root,
            &StoreRecipients::new(vec![key.fingerprint_hex.clone()]),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save store recipients");
        save_password_entry(
            &store_root,
            "team/empty-password",
            "\nusername: alice",
            true,
        )
        .expect("save password entry with empty first line");

        assert_eq!(
            super::read_password_entry(&store_root, "team/empty-password").expect("read entry"),
            "\nusername: alice"
        );
    }

    #[test]
    #[expect(
        clippy::significant_drop_tightening,
        reason = "SystemBackendTestEnv must stay alive for the full test to keep the temp store and env vars in place."
    )]
    fn host_backend_save_leaves_git_worktree_clean() {
        let env = SystemBackendTestEnv::new();
        env.init_store_git_repository()
            .expect("initialize store git repository");

        let key =
            SystemBackendTestEnv::generate_secret_key("Recipient <host-git-clean@example.com>")
                .expect("generate host recipient key");
        let secret_key_bytes = key
            .cert
            .as_tsk()
            .armored()
            .to_vec()
            .expect("serialize armored secret key");
        super::host_gpg_backend()
            .import_private_key_bytes(&secret_key_bytes)
            .expect("import host secret key");
        SystemBackendTestEnv::import_public_key(&key.public_key_bytes)
            .expect("import host recipient key");
        SystemBackendTestEnv::trust_public_key(&key.fingerprint_hex)
            .expect("trust host recipient key");

        let store_root = env.store_root().to_string_lossy().to_string();
        save_store_recipients(
            &store_root,
            &StoreRecipients::new(vec![key.fingerprint_hex.clone()]),
            StoreRecipientsPrivateKeyRequirement::AnyManagedKey,
        )
        .expect("save store recipients");
        save_password_entry(&store_root, "example/user", "secret\nusername: alice", true)
            .expect("save password entry");

        assert_eq!(
            env.store_git_status_porcelain()
                .expect("read store git status after host save"),
            ""
        );
    }
}
