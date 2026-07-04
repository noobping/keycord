#[cfg(feature = "fidokey")]
mod common;
#[cfg(not(feature = "fidokey"))]
mod disabled;
#[cfg(feature = "fidokey")]
mod key;
#[cfg(all(test, feature = "fidokey"))]
mod transport_test;

#[cfg(feature = "fidokey")]
pub use self::common::set_fido2_security_key_pin;
#[cfg(feature = "fidokey")]
pub(in crate::backend::integrated) use self::common::{
    Fido2DirectBinding, Fido2DirectBindingDescriptor,
};
#[cfg(feature = "fidokey")]
pub(in crate::backend::integrated) use self::key::{
    create_fido2_private_key_binding, encrypt_fido2_direct_required_layer,
    unlock_fido2_private_key_material_for_session,
};
#[cfg(all(test, feature = "fidokey"))]
pub(in crate::backend::integrated) use self::transport_test::{
    reset_fido2_transport_for_tests, set_fido2_transport_for_tests, Fido2AssertionOutput,
    Fido2DeviceLabel, Fido2Enrollment, Fido2Transport, Fido2TransportError,
};

#[cfg(not(feature = "fidokey"))]
pub use self::disabled::set_fido2_security_key_pin;
