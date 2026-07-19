mod cache;
mod cert;
mod errors;
#[cfg(feature = "fidokey")]
#[path = "fido2/mod.rs"]
mod fido2;
#[cfg(not(feature = "fidokey"))]
#[path = "fido2/mod.rs"]
mod fido2;
#[path = "hardware/mod.rs"]
mod hardware;
mod store;

#[cfg(test)]
pub(in crate::backend) use self::cache::clear_cached_unlocked_ripasso_private_keys;
pub(in crate::backend) use self::cache::clear_integrated_runtime_secret_state;
pub(in crate::backend::integrated) use self::cache::{
    borrow_unlocked_hardware_private_key, borrow_unlocked_ripasso_private_key,
};
pub(in crate::backend::integrated) use self::cert::fingerprint_from_string;
#[cfg(test)]
pub(in crate::backend::integrated) use self::cert::{
    parse_managed_private_key_bytes, prepare_managed_private_key_bytes,
};
pub use self::cert::{
    ConnectedSmartcardKey, ManagedRipassoHardwareKey, ManagedRipassoPrivateKey,
    ManagedRipassoPrivateKeyProtection, PrivateKeyUnlockKind, PrivateKeyUnlockRequest,
};
pub(in crate::backend::integrated) use self::errors::{
    password_entry_error_from_integrated_message,
    password_entry_write_error_from_integrated_message, password_entry_write_error_from_io,
    store_recipients_error_from_integrated_message, INCOMPATIBLE_PRIVATE_KEY_ERROR,
    LOCKED_PRIVATE_KEY_ERROR, MISSING_PRIVATE_KEY_ERROR,
};
#[cfg(all(test, feature = "fidokey"))]
pub(in crate::backend::integrated) use self::fido2::{
    reset_fido2_transport_for_tests, set_fido2_transport_for_tests, Fido2AssertionOutput,
    Fido2DeviceLabel, Fido2Enrollment, Fido2Transport, Fido2TransportError,
};
pub use self::hardware::DiscoveredHardwareToken;
pub(in crate::backend::integrated) use self::hardware::{
    decrypt_with_hardware_session, sign_with_hardware_session, HardwareSessionPolicy,
};
#[cfg(all(test, feature = "hardwarekey"))]
pub(in crate::backend::integrated) use self::hardware::{
    reset_hardware_transport_for_tests, set_hardware_transport_for_tests,
    HardwareKeyGenerationRequest, HardwareTransport, HardwareTransportError,
};
#[cfg(all(test, not(feature = "hardwarekey")))]
pub(in crate::backend::integrated) use self::hardware::{
    reset_hardware_transport_for_tests, set_hardware_transport_for_tests, HardwareTransport,
    HardwareTransportError,
};
#[cfg(feature = "audit")]
pub(in crate::backend) use self::store::available_standard_public_certs;
#[cfg(test)]
pub use self::store::resolved_ripasso_own_fingerprint;
#[cfg(test)]
pub(in crate::backend::integrated) use self::store::ripasso_keys_dir;
#[cfg(all(test, feature = "hardwarekey"))]
pub use self::store::store_ripasso_hardware_key_bytes;
#[cfg(target_os = "linux")]
pub use self::store::store_ripasso_private_key_bytes;
pub use self::store::{
    armored_ripasso_private_key, armored_ripasso_public_key, discover_ripasso_hardware_keys,
    generate_fido2_private_key, generate_ripasso_hardware_key, generate_ripasso_private_key,
    import_ripasso_hardware_key_bytes, import_ripasso_private_key_bytes,
    is_ripasso_private_key_unlocked, list_connected_smartcard_keys, list_ripasso_private_keys,
    remove_ripasso_private_key, ripasso_private_key_requires_passphrase,
    ripasso_private_key_requires_session_unlock, ripasso_private_key_title,
    set_fido2_security_key_pin, unlock_ripasso_private_key_for_session,
};
pub(in crate::backend::integrated) use self::store::{
    available_private_key_fingerprints, build_ripasso_crypto_from_key_ring,
    ensure_ripasso_private_key_is_ready, load_available_standard_key_ring, load_ripasso_key_ring,
    missing_private_key_error, selected_ripasso_own_fingerprint,
};
pub(crate) use self::store::{
    prepare_managed_private_key_storage_for_startup, ManagedKeyStorageStartup,
};
