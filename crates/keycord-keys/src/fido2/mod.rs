#[cfg(not(feature = "fido"))]
mod disabled;
#[cfg(feature = "fido")]
mod enabled;

#[cfg(not(feature = "fido"))]
pub use self::disabled::set_fido2_security_key_pin;
#[cfg(feature = "fido")]
pub use self::enabled::set_fido2_security_key_pin;
#[cfg(feature = "fido")]
pub(crate) use self::enabled::{clear_cached_fido2_secrets, remove_cached_fido2_secrets};
#[cfg(feature = "fido")]
pub(crate) use self::enabled::{
    create_fido2_private_key_binding, encrypt_fido2_direct_required_layer,
    unlock_fido2_private_key_material_for_session,
};
