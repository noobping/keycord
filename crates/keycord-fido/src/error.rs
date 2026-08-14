use crate::FidoTransportError;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FidoErrorKind {
    PinNotSet,
    PinRequired,
    IncorrectPin,
    PinUnsupported,
    TokenNotPresent,
    UserActionTimeout,
    TokenRemoved,
    Unsupported,
    InvalidData,
    Crypto,
    Other,
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum FidoError {
    #[error("Set a PIN on the FIDO2 security key first.")]
    PinNotSet,
    #[error("Enter the FIDO2 security key PIN.")]
    PinRequired,
    #[error("The FIDO2 security key PIN is incorrect.")]
    IncorrectPin,
    #[error("That FIDO2 security key must support PIN protection.")]
    PinUnsupported,
    #[error("Setting a FIDO2 security key PIN is only supported on Linux in this build.")]
    PinSetupUnavailable,
    #[error("Connect the matching FIDO2 security key.")]
    TokenNotPresent,
    #[error("Touch the FIDO2 security key and try again.")]
    UserActionTimeout,
    #[error("Reconnect the FIDO2 security key and try again.")]
    TokenRemoved,
    #[error("That FIDO2 security key does not support the hmac-secret extension.")]
    Unsupported,
    #[error("{0}")]
    InvalidData(String),
    #[error("{0}")]
    Crypto(String),
    #[error("{0}")]
    Other(String),
}

impl FidoError {
    pub const fn kind(&self) -> FidoErrorKind {
        match self {
            Self::PinNotSet => FidoErrorKind::PinNotSet,
            Self::PinRequired => FidoErrorKind::PinRequired,
            Self::IncorrectPin => FidoErrorKind::IncorrectPin,
            Self::PinUnsupported | Self::PinSetupUnavailable => FidoErrorKind::PinUnsupported,
            Self::TokenNotPresent => FidoErrorKind::TokenNotPresent,
            Self::UserActionTimeout => FidoErrorKind::UserActionTimeout,
            Self::TokenRemoved => FidoErrorKind::TokenRemoved,
            Self::Unsupported => FidoErrorKind::Unsupported,
            Self::InvalidData(_) => FidoErrorKind::InvalidData,
            Self::Crypto(_) => FidoErrorKind::Crypto,
            Self::Other(_) => FidoErrorKind::Other,
        }
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidData(message.into())
    }

    pub(crate) fn crypto(message: impl Into<String>) -> Self {
        Self::Crypto(message.into())
    }
}

impl From<FidoTransportError> for FidoError {
    fn from(value: FidoTransportError) -> Self {
        match value {
            FidoTransportError::PinNotSet => Self::PinNotSet,
            FidoTransportError::PinRequired => Self::PinRequired,
            FidoTransportError::IncorrectPin => Self::IncorrectPin,
            FidoTransportError::PinUnsupported => Self::PinUnsupported,
            FidoTransportError::TokenNotPresent => Self::TokenNotPresent,
            FidoTransportError::UserActionTimeout => Self::UserActionTimeout,
            FidoTransportError::TokenRemoved => Self::TokenRemoved,
            FidoTransportError::Unsupported => Self::Unsupported,
            FidoTransportError::Other(message) => Self::Other(message),
        }
    }
}
