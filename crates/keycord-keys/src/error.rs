use thiserror::Error;

/// Maps implementation-specific private-key failures to stable user-action guidance.
pub fn private_key_user_action_message(message: &str) -> Option<&'static str> {
    if message.contains("Enter the FIDO2 security key PIN.") {
        Some("Enter the FIDO2 security key PIN.")
    } else if message.contains("Set a PIN on the FIDO2 security key first.") {
        Some("Set a PIN on the FIDO2 security key first.")
    } else if message.contains("That FIDO2 security key must support PIN protection.") {
        Some("That FIDO2 security key must support PIN protection.")
    } else if message.contains("Touch the FIDO2 security key and try again.") {
        Some("Touch the FIDO2 security key and try again.")
    } else if message.contains("Reconnect the FIDO2 security key and try again.") {
        Some("Reconnect the FIDO2 security key and try again.")
    } else if message.contains("Connect the matching FIDO2 security key.") {
        Some("Connect the matching FIDO2 security key.")
    } else if message
        .contains("That FIDO2 security key does not support the hmac-secret extension.")
    {
        Some("That FIDO2 security key does not support the hmac-secret extension.")
    } else {
        None
    }
}

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
    #[cfg(feature = "fido")]
    #[error("{0}")]
    Fido2TokenNotPresent(String),
    #[cfg(feature = "fido")]
    #[error("{0}")]
    Fido2PinNotSet(String),
    #[cfg(feature = "fido")]
    #[error("{0}")]
    Fido2PinRequired(String),
    #[cfg(feature = "fido")]
    #[error("{0}")]
    IncorrectFido2Pin(String),
    #[cfg(feature = "fido")]
    #[error("{0}")]
    Fido2PinUnsupported(String),
    #[error("{0}")]
    UnsupportedFido2Key(String),
    #[cfg(feature = "fido")]
    #[error("{0}")]
    Fido2UserActionTimeout(String),
    #[cfg(feature = "fido")]
    #[error("{0}")]
    Fido2TokenRemoved(String),
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
    #[cfg(feature = "fido")]
    pub fn fido2_token_not_present(message: impl Into<String>) -> Self {
        Self::Fido2TokenNotPresent(message.into())
    }
    #[cfg(feature = "fido")]
    pub fn fido2_pin_not_set(message: impl Into<String>) -> Self {
        Self::Fido2PinNotSet(message.into())
    }
    #[cfg(feature = "fido")]
    pub fn fido2_pin_required(message: impl Into<String>) -> Self {
        Self::Fido2PinRequired(message.into())
    }
    #[cfg(feature = "fido")]
    pub fn incorrect_fido2_pin(message: impl Into<String>) -> Self {
        Self::IncorrectFido2Pin(message.into())
    }
    #[cfg(feature = "fido")]
    pub fn fido2_pin_unsupported(message: impl Into<String>) -> Self {
        Self::Fido2PinUnsupported(message.into())
    }
    pub fn unsupported_fido2_key(message: impl Into<String>) -> Self {
        Self::UnsupportedFido2Key(message.into())
    }
    #[cfg(feature = "fido")]
    pub fn fido2_user_action_timeout(message: impl Into<String>) -> Self {
        Self::Fido2UserActionTimeout(message.into())
    }
    #[cfg(feature = "fido")]
    pub fn fido2_token_removed(message: impl Into<String>) -> Self {
        Self::Fido2TokenRemoved(message.into())
    }
    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    /// Translate Keys' protection adapter error back to FIDO retry policy.
    #[cfg(feature = "fido")]
    pub const fn fido_error_kind(&self) -> Option<keycord_fido::FidoErrorKind> {
        use keycord_fido::FidoErrorKind;

        match self {
            Self::Fido2TokenNotPresent(_) => Some(FidoErrorKind::TokenNotPresent),
            Self::Fido2PinNotSet(_) => Some(FidoErrorKind::PinNotSet),
            Self::Fido2PinRequired(_) => Some(FidoErrorKind::PinRequired),
            Self::IncorrectFido2Pin(_) => Some(FidoErrorKind::IncorrectPin),
            Self::Fido2PinUnsupported(_) => Some(FidoErrorKind::PinUnsupported),
            Self::UnsupportedFido2Key(_) => Some(FidoErrorKind::Unsupported),
            Self::Fido2UserActionTimeout(_) => Some(FidoErrorKind::UserActionTimeout),
            Self::Fido2TokenRemoved(_) => Some(FidoErrorKind::TokenRemoved),
            _ => None,
        }
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
            #[cfg(feature = "fido")]
            Self::Fido2TokenNotPresent(_) => "Connect the FIDO2 security key and try again.",
            #[cfg(feature = "fido")]
            Self::Fido2PinNotSet(_) => "Set a PIN on the FIDO2 security key first.",
            #[cfg(feature = "fido")]
            Self::Fido2PinRequired(_) | Self::IncorrectFido2Pin(_) => {
                "Couldn't unlock the FIDO2 security key."
            }
            #[cfg(feature = "fido")]
            Self::Fido2PinUnsupported(_) => "That FIDO2 security key must support PIN protection.",
            Self::UnsupportedFido2Key(_) => "This FIDO2 security key can't open your items.",
            #[cfg(feature = "fido")]
            Self::Fido2UserActionTimeout(_) => "Touch the FIDO2 security key and try again.",
            #[cfg(feature = "fido")]
            Self::Fido2TokenRemoved(_) => "Reconnect the FIDO2 security key and try again.",
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
            #[cfg(feature = "fido")]
            Self::Fido2TokenNotPresent(_) => "Connect the FIDO2 security key first.",
            #[cfg(feature = "fido")]
            Self::Fido2PinNotSet(_) => "Set a PIN on the FIDO2 security key first.",
            #[cfg(feature = "fido")]
            Self::Fido2PinRequired(_) | Self::IncorrectFido2Pin(_) => {
                "Couldn't unlock the FIDO2 security key."
            }
            #[cfg(feature = "fido")]
            Self::Fido2PinUnsupported(_) => "That FIDO2 security key must support PIN protection.",
            Self::UnsupportedFido2Key(_) => "This FIDO2 security key can't open your items.",
            #[cfg(feature = "fido")]
            Self::Fido2UserActionTimeout(_) => "Touch the FIDO2 security key and try again.",
            #[cfg(feature = "fido")]
            Self::Fido2TokenRemoved(_) => "Reconnect the FIDO2 security key and try again.",
            Self::PassphraseRequired(_) | Self::IncorrectPassphrase(_) => {
                "Couldn't unlock the key."
            }
            Self::Other(message)
                if message.contains("Connect only one FIDO2 security key before continuing.") =>
            {
                "Unplug the other security keys, then try again."
            }
            Self::Other(message) => {
                private_key_user_action_message(message).unwrap_or("Couldn't import the key.")
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

#[cfg(test)]
mod tests {
    use super::PrivateKeyError;

    #[test]
    fn fido_import_guidance_is_preserved() {
        assert_eq!(
            PrivateKeyError::other("Connect only one FIDO2 security key before continuing.")
                .import_message(),
            "Unplug the other security keys, then try again."
        );
    }
}
