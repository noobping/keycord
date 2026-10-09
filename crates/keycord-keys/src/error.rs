use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PrivateKeyError {
    #[error("{0}")]
    NotStored(String),
    #[error("{0}")]
    MissingPrivateKeyMaterial(String),
    #[error("{0}")]
    PassphraseRequired(String),
    #[error("{0}")]
    IncorrectPassphrase(String),
    #[error("{0}")]
    RequiresPasswordProtection(String),
    #[error("{0}")]
    Incompatible(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwareTokenNotPresent(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwareTokenMismatch(String),
    #[error("{0}")]
    HardwarePinRequired(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    IncorrectHardwarePin(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwarePinBlocked(String),
    #[error("{0}")]
    UnsupportedHardwareKey(String),
    #[cfg(feature = "smartcard")]
    #[error("{0}")]
    HardwareTokenRemoved(String),
    #[error("{0}")]
    Other(String),
}

impl PrivateKeyError {
    pub fn not_stored(message: impl Into<String>) -> Self {
        Self::NotStored(message.into())
    }
    pub fn missing_private_key_material(message: impl Into<String>) -> Self {
        Self::MissingPrivateKeyMaterial(message.into())
    }
    pub fn passphrase_required(message: impl Into<String>) -> Self {
        Self::PassphraseRequired(message.into())
    }
    pub fn incorrect_passphrase(message: impl Into<String>) -> Self {
        Self::IncorrectPassphrase(message.into())
    }
    pub fn requires_password_protection(message: impl Into<String>) -> Self {
        Self::RequiresPasswordProtection(message.into())
    }
    pub fn incompatible(message: impl Into<String>) -> Self {
        Self::Incompatible(message.into())
    }
    #[cfg(feature = "smartcard")]
    pub fn hardware_token_not_present(message: impl Into<String>) -> Self {
        Self::HardwareTokenNotPresent(message.into())
    }
    #[cfg(feature = "smartcard")]
    pub fn hardware_token_mismatch(message: impl Into<String>) -> Self {
        Self::HardwareTokenMismatch(message.into())
    }
    pub fn hardware_pin_required(message: impl Into<String>) -> Self {
        Self::HardwarePinRequired(message.into())
    }
    #[cfg(feature = "smartcard")]
    pub fn incorrect_hardware_pin(message: impl Into<String>) -> Self {
        Self::IncorrectHardwarePin(message.into())
    }
    #[cfg(feature = "smartcard")]
    pub fn hardware_pin_blocked(message: impl Into<String>) -> Self {
        Self::HardwarePinBlocked(message.into())
    }
    pub fn unsupported_hardware_key(message: impl Into<String>) -> Self {
        Self::UnsupportedHardwareKey(message.into())
    }
    #[cfg(feature = "smartcard")]
    pub fn hardware_token_removed(message: impl Into<String>) -> Self {
        Self::HardwareTokenRemoved(message.into())
    }
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub const fn unlock_message(&self) -> &'static str {
        match self {
            Self::Incompatible(_) => "This key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenNotPresent(_) => "Connect the hardware key and try again.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenMismatch(_) => "Use the matching hardware key.",
            #[cfg(feature = "smartcard")]
            Self::HardwarePinRequired(_)
            | Self::IncorrectHardwarePin(_)
            | Self::HardwarePinBlocked(_) => "Couldn't unlock the hardware key.",
            #[cfg(not(feature = "smartcard"))]
            Self::HardwarePinRequired(_) => "Couldn't unlock the hardware key.",
            Self::UnsupportedHardwareKey(_) => "This hardware key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenRemoved(_) => "Reconnect the hardware key and try again.",
            _ => "Couldn't unlock the key.",
        }
    }

    pub fn import_message(&self) -> &'static str {
        match self {
            Self::MissingPrivateKeyMaterial(_) => "That file does not contain a private key.",
            Self::RequiresPasswordProtection(_) => "Add a password to that key first.",
            Self::Incompatible(_) => "This key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenNotPresent(_) => "Connect the hardware key first.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenMismatch(_) => "Use the matching hardware key.",
            #[cfg(feature = "smartcard")]
            Self::HardwarePinRequired(_) | Self::IncorrectHardwarePin(_) => {
                "Couldn't unlock the hardware key."
            }
            #[cfg(not(feature = "smartcard"))]
            Self::HardwarePinRequired(_) => "Couldn't unlock the hardware key.",
            #[cfg(feature = "smartcard")]
            Self::HardwarePinBlocked(_) => "The hardware key PIN is blocked.",
            Self::UnsupportedHardwareKey(_) => "This hardware key can't open your items.",
            #[cfg(feature = "smartcard")]
            Self::HardwareTokenRemoved(_) => "Reconnect the hardware key and try again.",
            Self::PassphraseRequired(_) | Self::IncorrectPassphrase(_) => {
                "Couldn't unlock the key."
            }
            _ => "Couldn't import the key.",
        }
    }

    pub const fn inspection_message(&self) -> &'static str {
        match self {
            Self::MissingPrivateKeyMaterial(_) => "That data does not contain a private key.",
            _ => "Couldn't read that key.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PrivateKeyReadinessError {
    #[error("{0}")]
    Missing(String),
    #[error("{0}")]
    Locked(String),
    #[error("{0}")]
    Incompatible(String),
    #[error("{0}")]
    Other(String),
}

impl PrivateKeyReadinessError {
    pub fn missing(message: impl Into<String>) -> Self {
        Self::Missing(message.into())
    }
    pub fn locked(message: impl Into<String>) -> Self {
        Self::Locked(message.into())
    }
    pub fn incompatible(message: impl Into<String>) -> Self {
        Self::Incompatible(message.into())
    }
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }
}
