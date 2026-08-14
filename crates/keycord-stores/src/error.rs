use thiserror::Error;

use keycord_keys::{
    private_key_user_action_message, INCOMPATIBLE_PRIVATE_KEY_ERROR, LOCKED_PRIVATE_KEY_ERROR,
    MISSING_PRIVATE_KEY_ERROR,
};

pub const INVALID_STORE_PATH_ERROR: &str = "The selected password store path is not a folder.";

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum StoreRecipientsError {
    #[error("{0}")]
    InvalidStorePath(String),
    #[error("{0}")]
    MissingPrivateKey(String),
    #[error("{0}")]
    LockedPrivateKey(String),
    #[error("{0}")]
    IncompatiblePrivateKey(String),
    #[error("{0}")]
    Other(String),
}

impl StoreRecipientsError {
    pub fn invalid_store_path(message: impl Into<String>) -> Self {
        Self::InvalidStorePath(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub fn toast_message(&self, fallback: &'static str) -> &'static str {
        match self {
            Self::InvalidStorePath(_) => "The selected store path is not a folder.",
            Self::MissingPrivateKey(_) => "Add a private key in Preferences.",
            Self::LockedPrivateKey(_) => "Unlock the key in Preferences.",
            Self::IncompatiblePrivateKey(_) => "This key can't open your items.",
            Self::Other(message) => private_key_user_action_message(message).unwrap_or(fallback),
        }
    }
}

/// Classifies an integrated-backend failure without leaking backend details to callers.
pub fn store_recipients_error_from_integrated_message(
    message: impl Into<String>,
) -> StoreRecipientsError {
    let message = message.into();
    match message.as_str() {
        INVALID_STORE_PATH_ERROR => StoreRecipientsError::invalid_store_path(message),
        MISSING_PRIVATE_KEY_ERROR => StoreRecipientsError::MissingPrivateKey(message),
        LOCKED_PRIVATE_KEY_ERROR => StoreRecipientsError::LockedPrivateKey(message),
        INCOMPATIBLE_PRIVATE_KEY_ERROR => StoreRecipientsError::IncompatiblePrivateKey(message),
        _ => StoreRecipientsError::other(message),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        store_recipients_error_from_integrated_message, StoreRecipientsError,
        INVALID_STORE_PATH_ERROR,
    };
    use keycord_keys::LOCKED_PRIVATE_KEY_ERROR;

    #[test]
    fn store_recipient_errors_use_specific_toasts_when_available() {
        assert_eq!(
            StoreRecipientsError::MissingPrivateKey("missing".to_string())
                .toast_message("Couldn't save recipients."),
            "Add a private key in Preferences."
        );
        assert_eq!(
            StoreRecipientsError::invalid_store_path("invalid".to_string())
                .toast_message("Couldn't create the store."),
            "The selected store path is not a folder."
        );
    }

    #[test]
    fn integrated_messages_map_to_store_recipient_variants() {
        let invalid = store_recipients_error_from_integrated_message(INVALID_STORE_PATH_ERROR);
        assert!(matches!(invalid, StoreRecipientsError::InvalidStorePath(_)));
        assert_eq!(
            invalid.toast_message("Couldn't save recipients."),
            "The selected store path is not a folder."
        );

        let locked = store_recipients_error_from_integrated_message(LOCKED_PRIVATE_KEY_ERROR);
        assert!(matches!(locked, StoreRecipientsError::LockedPrivateKey(_)));
        assert_eq!(
            locked.toast_message("Couldn't save recipients."),
            "Unlock the key in Preferences."
        );
    }
}
