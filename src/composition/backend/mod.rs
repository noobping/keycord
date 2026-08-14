mod command;
mod host;
mod integrated;
#[cfg(test)]
mod test_support;

#[cfg(feature = "audit")]
use sequoia_openpgp::Cert;

use keycord_entries::{PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError};
use keycord_keys::ManagedKeyStorageStartup as StartupPreparation;
use keycord_stores::{StoreRecipients, StoreRecipientsError, StoreRecipientsPrivateKeyRequirement};

#[cfg(target_os = "linux")]
pub(crate) use self::host::host_gpg_backend;
#[cfg(test)]
pub use integrated::required_private_key_fingerprints_for_entry;
pub use integrated::{
    git_commit_private_key_requiring_unlock_for_entry,
    git_commit_private_key_requiring_unlock_for_store_recipients,
};

#[cfg(feature = "audit")]
pub fn available_standard_public_certs() -> Result<Vec<Cert>, String> {
    keycord_keys::available_standard_public_certs()
}

#[cfg(feature = "audit")]
pub fn available_host_gpg_public_certs() -> Result<Vec<Cert>, String> {
    #[cfg(target_os = "linux")]
    {
        host::available_host_gpg_public_certs()
    }

    #[cfg(not(target_os = "linux"))]
    {
        Ok(Vec::new())
    }
}

use keycord_preferences::Preferences;

fn dispatch_backend<T>(integrated: impl FnOnce() -> T, host: impl FnOnce() -> T) -> T {
    if Preferences::new().uses_integrated_backend() {
        integrated()
    } else {
        host()
    }
}

macro_rules! dispatch_backend_call {
    ($(fn $name:ident($($arg:ident: $arg_ty:ty),* $(,)?) -> $ret:ty;)+) => {
        $(
            pub fn $name($($arg: $arg_ty),*) -> $ret {
                dispatch_backend(
                    || integrated::$name($($arg),*),
                    || host::$name($($arg),*),
                )
            }
        )+
    };
}

dispatch_backend_call! {
    fn read_password_entry(store_root: &str, label: &str) -> Result<String, PasswordEntryError>;
    fn read_password_line(store_root: &str, label: &str) -> Result<String, PasswordEntryError>;
    fn save_password_entry(
        store_root: &str,
        label: &str,
        contents: &str,
        overwrite: bool,
    ) -> Result<(), PasswordEntryWriteError>;
    fn rename_password_entry(
        store_root: &str,
        old_label: &str,
        new_label: &str,
    ) -> Result<(), PasswordEntryWriteError>;
    fn delete_password_entry(store_root: &str, label: &str) -> Result<(), PasswordEntryWriteError>;
    fn save_store_recipients(
        store_root: &str,
        recipients: &StoreRecipients,
        private_key_requirement: StoreRecipientsPrivateKeyRequirement,
    ) -> Result<(), StoreRecipientsError>;
}

pub fn save_store_recipients_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
    recipients: &StoreRecipients,
    private_key_requirement: StoreRecipientsPrivateKeyRequirement,
) -> Result<(), StoreRecipientsError> {
    dispatch_backend(
        || {
            integrated::save_store_recipients_for_relative_dir(
                store_root,
                relative_dir,
                recipients,
                private_key_requirement,
            )
        },
        || {
            host::save_store_recipients_for_relative_dir(
                store_root,
                relative_dir,
                recipients,
                private_key_requirement,
            )
        },
    )
}

pub fn read_password_entry_with_progress(
    store_root: &str,
    label: &str,
    report_progress: &mut dyn FnMut(PasswordEntryReadProgress),
) -> Result<String, PasswordEntryError> {
    if Preferences::new().uses_integrated_backend() {
        integrated::read_password_entry_with_progress(store_root, label, report_progress)
    } else {
        host::read_password_entry_with_progress(store_root, label)
    }
}

pub fn password_entry_is_readable(store_root: &str, label: &str) -> bool {
    dispatch_backend(
        || integrated::password_entry_is_readable(store_root, label),
        || host::password_entry_is_readable(store_root, label),
    )
}

pub fn store_recipients_private_key_requiring_unlock(
    store_root: &str,
) -> Result<Option<String>, String> {
    dispatch_backend(
        || integrated::store_recipients_private_key_requiring_unlock(store_root),
        || host::store_recipients_private_key_requiring_unlock(store_root),
    )
}

pub fn store_recipients_private_key_requiring_unlock_for_relative_dir(
    store_root: &str,
    relative_dir: &str,
) -> Result<Option<String>, String> {
    dispatch_backend(
        || {
            integrated::store_recipients_private_key_requiring_unlock_for_relative_dir(
                store_root,
                relative_dir,
            )
        },
        || {
            host::store_recipients_private_key_requiring_unlock_for_relative_dir(
                store_root,
                relative_dir,
            )
        },
    )
}

pub fn clear_runtime_secret_state() {
    keycord_keys::clear_integrated_runtime_secret_state();
}

pub(crate) fn prepare_startup() -> Result<StartupPreparation, String> {
    if Preferences::new().uses_integrated_backend() {
        keycord_keys::prepare_managed_private_key_storage_for_startup()
    } else {
        Ok(StartupPreparation::Ready)
    }
}
