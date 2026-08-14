//! OpenPGP key management for Keycord.
//!
//! This crate owns managed OpenPGP certificates, private-key protection,
//! smartcard/FIDO adapters, cache lifetime, and the established key files on
//! disk. Password entries, stores, and Git are consumers through the public
//! key-ring and readiness APIs; they are not dependencies of this subject.

mod cache;
mod capabilities;
mod cert;
mod error;
mod fido2;
mod hardware;
mod host_gpg;
mod store;
mod sync;

#[cfg(feature = "ui")]
pub mod ui;

#[cfg(test)]
mod test_support {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    pub(crate) fn lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

pub const MISSING_PRIVATE_KEY_ERROR: &str =
    "Import a private key in Preferences before using the password store.";
pub const LOCKED_PRIVATE_KEY_ERROR: &str =
    "A private key for this item is locked. Unlock it in Preferences.";
pub const INCOMPATIBLE_PRIVATE_KEY_ERROR: &str =
    "The available private keys cannot decrypt this item.";

pub use capabilities::{
    hardware_key_available, has_smartcard_permission, host_private_key_sync_available,
    smartcard_available,
};

#[cfg(any(test, feature = "test-support"))]
pub use cache::clear_cached_unlocked_ripasso_private_keys;
pub use cache::{
    borrow_unlocked_hardware_private_key, borrow_unlocked_ripasso_private_key,
    clear_integrated_runtime_secret_state,
};
pub use cert::{
    cert_can_decrypt_password_entries, cert_has_transport_encryption_key, fingerprint_from_string,
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes, ConnectedSmartcardKey,
    ManagedRipassoHardwareKey, ManagedRipassoPrivateKey, ManagedRipassoPrivateKeyProtection,
    PrivateKeyUnlockKind, PrivateKeyUnlockRequest,
};
pub use error::{private_key_user_action_message, PrivateKeyError, PrivateKeyReadinessError};
pub use fido2::set_fido2_security_key_pin;
pub use hardware::{
    decrypt_with_hardware_session, sign_with_hardware_session, DiscoveredHardwareToken,
    HardwareSessionPolicy, HardwareTransport, HardwareTransportError,
};
pub use host_gpg::{
    HostGpgBackend, HostGpgCommand, HostGpgCommandOutput, HostGpgCommandPort,
    HostGpgPrivateKeySummary,
};
#[cfg(feature = "audit")]
pub use store::available_standard_public_certs;
#[cfg(all(any(test, feature = "test-support"), feature = "hardwarekey"))]
pub use store::store_ripasso_hardware_key_bytes;
#[cfg(target_os = "linux")]
pub use store::store_ripasso_private_key_bytes;
pub use store::{
    armored_managed_key_material, armored_ripasso_private_key, armored_ripasso_public_key,
    available_private_key_fingerprints, build_ripasso_crypto_from_key_ring,
    discover_ripasso_hardware_keys, ensure_ripasso_private_key_is_ready,
    generate_fido2_private_key, generate_ripasso_hardware_key, generate_ripasso_private_key,
    import_ripasso_hardware_key_bytes, import_ripasso_private_key_bytes,
    import_ripasso_private_key_with_secret, incompatible_private_key_error,
    is_ripasso_private_key_unlocked, list_connected_smartcard_keys, list_ripasso_private_keys,
    load_available_standard_key_ring, load_ripasso_key_ring, locked_private_key_error,
    missing_private_key_error, prepare_managed_private_key_storage_for_startup,
    remove_ripasso_private_key, ripasso_keys_dir, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ripasso_private_key_title,
    selected_ripasso_own_fingerprint, unlock_ripasso_private_key_for_session,
    ManagedKeyStorageStartup,
};
pub use sync::{
    preflight_host_to_app_private_key_sync, sync_private_keys_with_host, HostPrivateKeySyncPort,
    PrivateKeySyncDirection,
};

/// Crypto context constructed from Keys-owned managed key material for entry encryption.
pub type RipassoCrypto = ripasso::crypto::Sequoia;

#[cfg(any(test, feature = "test-support"))]
pub mod testing {
    #[cfg(feature = "hardwarekey")]
    pub use crate::hardware::HardwareKeyGenerationRequest;
    pub use crate::hardware::{
        reset_hardware_transport_for_tests, set_hardware_transport_for_tests, HardwareTransport,
        HardwareTransportError,
    };
    pub use crate::store::resolved_ripasso_own_fingerprint;
    #[cfg(feature = "hardwarekey")]
    pub use crate::store::store_ripasso_hardware_key_bytes;
}
