//! FIDO2 `hmac-secret` protection used by Keycord.
//!
//! The crate deliberately knows nothing about OpenPGP or password-store models.
//! Callers provide opaque private-key bytes and validate the public-key material
//! in [`FidoPrivateKeyManifest`] in their own domain.

mod cache;
mod capabilities;
mod crypto;
mod envelope;
mod error;
mod manifest;
#[cfg(feature = "native-transport")]
mod native;
mod service;
mod transport;
#[cfg(feature = "ui")]
pub mod ui;

pub use capabilities::{has_usb_permission, security_key_available};
pub use envelope::{FidoBinding, FidoBindingDescriptor, FIDO_REQUIRED_LAYER_HEADER};
pub use error::{FidoError, FidoErrorKind};
pub use manifest::{
    FidoPrivateKeyManifest, FIDO_PRIVATE_KEY_MANIFEST_FORMAT, FIDO_PRIVATE_KEY_PROTECTION_KIND,
};
#[cfg(feature = "native-transport")]
pub use native::NativeFidoTransport;
#[cfg(feature = "native-transport")]
pub use service::shared_native_service;
#[cfg(all(feature = "native-transport", feature = "test-support"))]
pub use service::{reset_shared_native_transport_for_tests, set_shared_native_transport_for_tests};
pub use service::{FidoService, RetryPolicy, FIDO_RP_ID};
pub use transport::{
    FidoAssertion, FidoDeviceLabel, FidoEnrollment, FidoTransport, FidoTransportError,
};
