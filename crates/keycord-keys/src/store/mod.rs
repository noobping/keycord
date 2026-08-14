mod manifest;
mod paths;
mod storage;
mod unlock;

use crate::{INCOMPATIBLE_PRIVATE_KEY_ERROR, LOCKED_PRIVATE_KEY_ERROR, MISSING_PRIVATE_KEY_ERROR};

const PRIVATE_KEY_NOT_STORED_ERROR: &str = "That private key is not stored in the app.";
#[cfg(not(feature = "fido"))]
const FIDO2_PRIVATE_KEY_FEATURE_DISABLED_ERROR: &str =
    "FIDO2 private-key support is disabled in this build of Keycord.";

pub fn missing_private_key_error() -> String {
    MISSING_PRIVATE_KEY_ERROR.to_string()
}

pub fn locked_private_key_error() -> String {
    LOCKED_PRIVATE_KEY_ERROR.to_string()
}

pub fn incompatible_private_key_error() -> String {
    INCOMPATIBLE_PRIVATE_KEY_ERROR.to_string()
}

fn private_key_not_stored_error() -> String {
    PRIVATE_KEY_NOT_STORED_ERROR.to_string()
}

#[cfg(all(any(test, feature = "test-support"), feature = "hardwarekey"))]
pub use storage::store_ripasso_hardware_key_bytes;
#[cfg(target_os = "linux")]
pub use storage::store_ripasso_private_key_bytes;
pub use storage::{
    armored_ripasso_private_key, armored_ripasso_public_key, discover_ripasso_hardware_keys,
    generate_fido2_private_key, generate_ripasso_hardware_key, generate_ripasso_private_key,
    import_ripasso_hardware_key_bytes, import_ripasso_private_key_bytes,
    import_ripasso_private_key_with_secret, list_connected_smartcard_keys,
    list_ripasso_private_keys, remove_ripasso_private_key, ripasso_private_key_title,
};

pub fn armored_managed_key_material(
    key: &crate::ManagedRipassoPrivateKey,
) -> Result<String, String> {
    match key.protection {
        crate::ManagedRipassoPrivateKeyProtection::Password => {
            armored_ripasso_private_key(&key.fingerprint)
        }
        crate::ManagedRipassoPrivateKeyProtection::HardwareOpenPgpCard => {
            armored_ripasso_public_key(&key.fingerprint)
        }
        #[cfg(feature = "fido")]
        crate::ManagedRipassoPrivateKeyProtection::Fido2HmacSecret => {
            armored_ripasso_private_key(&key.fingerprint)
        }
    }
}
pub use unlock::{
    is_ripasso_private_key_unlocked, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, unlock_ripasso_private_key_for_session,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManagedKeyStorageStartup {
    Ready,
}

pub fn prepare_managed_private_key_storage_for_startup() -> Result<ManagedKeyStorageStartup, String>
{
    Ok(ManagedKeyStorageStartup::Ready)
}

pub use paths::ripasso_keys_dir;
#[cfg(feature = "audit")]
pub use storage::available_standard_public_certs;
#[cfg(any(test, feature = "test-support"))]
pub use storage::resolved_ripasso_own_fingerprint;
pub use storage::{
    available_private_key_fingerprints, build_ripasso_crypto_from_key_ring,
    load_available_standard_key_ring, load_ripasso_key_ring, selected_ripasso_own_fingerprint,
};
pub use unlock::ensure_ripasso_private_key_is_ready;
