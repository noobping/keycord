//! Password entry workflows and presentation for Keycord.

#[cfg(feature = "ui")]
pub mod clipboard;
mod error;
pub mod file;
pub mod generation;
pub mod host;
pub mod import;
pub mod integrated;
pub mod launch;
pub mod model;
#[cfg(feature = "ui")]
pub mod otp;
pub mod search;
pub mod strength;
pub mod tools;
#[cfg(feature = "ui")]
pub mod ui;
pub mod undo;
pub mod validation;

pub use error::{
    PasswordEntryError, PasswordEntryProgress, PasswordEntryReadProgress, PasswordEntryWriteError,
    PasswordEntryWriteProgress,
};
