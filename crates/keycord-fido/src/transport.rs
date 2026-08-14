use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidoDeviceLabel {
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FidoTransportError {
    PinNotSet,
    PinRequired,
    IncorrectPin,
    PinUnsupported,
    TokenNotPresent,
    UserActionTimeout,
    TokenRemoved,
    Unsupported,
    Other(String),
}

impl Display for FidoTransportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PinNotSet => write!(f, "Set a PIN on the FIDO2 security key first."),
            Self::PinRequired => write!(f, "Enter the FIDO2 security key PIN."),
            Self::IncorrectPin => write!(f, "The FIDO2 security key PIN is incorrect."),
            Self::PinUnsupported => {
                write!(f, "That FIDO2 security key must support PIN protection.")
            }
            Self::TokenNotPresent => write!(f, "Connect the matching FIDO2 security key."),
            Self::UserActionTimeout => write!(f, "Touch the FIDO2 security key and try again."),
            Self::TokenRemoved => write!(f, "Reconnect the FIDO2 security key and try again."),
            Self::Unsupported => write!(
                f,
                "That FIDO2 security key does not support the hmac-secret extension."
            ),
            Self::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for FidoTransportError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidoEnrollment {
    pub credential_id: Vec<u8>,
    pub device: FidoDeviceLabel,
    pub hmac_secret: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidoAssertion {
    pub hmac_secret: Vec<u8>,
    pub device: Option<FidoDeviceLabel>,
}

/// Hardware boundary for FIDO operations.
///
/// Tests and alternative frontends can inject a transport without replacing
/// process-global state.
pub trait FidoTransport: Send + Sync {
    fn enroll_hmac_secret(
        &self,
        rp_id: &str,
        user_name: &str,
        user_display_name: &str,
        pin: Option<&str>,
        salt: &[u8],
    ) -> Result<FidoEnrollment, FidoTransportError>;

    fn derive_hmac_secret(
        &self,
        rp_id: &str,
        credential_id: &[u8],
        pin: Option<&str>,
        salt: &[u8],
        excluded_devices: &[FidoDeviceLabel],
    ) -> Result<FidoAssertion, FidoTransportError>;

    fn set_new_pin(&self, _new_pin: &str) -> Result<(), FidoTransportError> {
        Err(FidoTransportError::PinUnsupported)
    }
}
