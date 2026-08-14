//! Canonical password-entry operation errors and progress values.

use keycord_keys::private_key_user_action_message;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PasswordEntryError {
    #[error("{0}")]
    EntryNotFound(String),
    #[error("{0}")]
    MissingPrivateKey(String),
    #[error("{0}")]
    LockedPrivateKey(String),
    #[error("{0}")]
    IncompatiblePrivateKey(String),
    #[error("{0}")]
    Other(String),
}

impl PasswordEntryError {
    pub fn missing_private_key(message: impl Into<String>) -> Self {
        Self::MissingPrivateKey(message.into())
    }

    pub fn locked_private_key(message: impl Into<String>) -> Self {
        Self::LockedPrivateKey(message.into())
    }

    pub fn incompatible_private_key(message: impl Into<String>) -> Self {
        Self::IncompatiblePrivateKey(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub const fn toast_message(&self) -> Option<&'static str> {
        match self {
            Self::MissingPrivateKey(_) => Some("Add a private key in Preferences."),
            Self::IncompatiblePrivateKey(_) => Some("This key can't open your items."),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PasswordEntryWriteError {
    #[error("{0}")]
    EntryAlreadyExists(String),
    #[error("{0}")]
    EntryNotFound(String),
    #[error("{0}")]
    MissingPrivateKey(String),
    #[error("{0}")]
    LockedPrivateKey(String),
    #[error("{0}")]
    IncompatiblePrivateKey(String),
    #[error("{0}")]
    Other(String),
}

impl PasswordEntryWriteError {
    pub fn already_exists(message: impl Into<String>) -> Self {
        Self::EntryAlreadyExists(message.into())
    }

    pub fn entry_not_found(message: impl Into<String>) -> Self {
        Self::EntryNotFound(message.into())
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::Other(message.into())
    }

    pub fn save_toast_message(&self) -> &'static str {
        match self {
            Self::EntryAlreadyExists(_) => "An item with that name already exists.",
            Self::MissingPrivateKey(_) => "Add a private key in Preferences.",
            Self::LockedPrivateKey(_) => "Unlock the key in Preferences.",
            Self::IncompatiblePrivateKey(_) => "This key can't open your items.",
            Self::Other(message) => {
                private_key_user_action_message(message).unwrap_or("Couldn't save changes.")
            }
            Self::EntryNotFound(_) => "Couldn't save changes.",
        }
    }

    pub const fn rename_toast_message(&self) -> &'static str {
        match self {
            Self::EntryAlreadyExists(_) => "An item with that name already exists.",
            Self::EntryNotFound(_) => "That item no longer exists.",
            Self::MissingPrivateKey(_)
            | Self::LockedPrivateKey(_)
            | Self::IncompatiblePrivateKey(_)
            | Self::Other(_) => "Couldn't rename the item.",
        }
    }

    pub const fn delete_toast_message(&self) -> &'static str {
        match self {
            Self::EntryNotFound(_) => "That item no longer exists.",
            Self::EntryAlreadyExists(_)
            | Self::MissingPrivateKey(_)
            | Self::LockedPrivateKey(_)
            | Self::IncompatiblePrivateKey(_)
            | Self::Other(_) => "Couldn't delete the item.",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordEntryProgress {
    pub current_step: usize,
    pub total_steps: usize,
}

pub type PasswordEntryReadProgress = PasswordEntryProgress;
pub type PasswordEntryWriteProgress = PasswordEntryProgress;

#[cfg(test)]
mod tests {
    use super::PasswordEntryWriteError;

    #[test]
    fn write_errors_map_to_user_toasts() {
        assert_eq!(
            PasswordEntryWriteError::EntryAlreadyExists("duplicate".to_string())
                .save_toast_message(),
            "An item with that name already exists."
        );
        assert_eq!(
            PasswordEntryWriteError::EntryNotFound("missing".to_string()).delete_toast_message(),
            "That item no longer exists."
        );
        assert_eq!(
            PasswordEntryWriteError::Other(
                "Touch the FIDO2 security key and try again.".to_string()
            )
            .save_toast_message(),
            "Touch the FIDO2 security key and try again."
        );
    }
}
