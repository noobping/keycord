//! Preferences persistence and presentation for Keycord.

mod command_backend;
mod command_logging;
mod generation;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod non_linux;
mod preferences;
mod restricted;
mod storage;

#[cfg(feature = "ui")]
pub mod ui;

pub use command_logging::password_store_command_log_options;
pub use generation::PasswordGenerationSettings;
pub use preferences::{BackendKind, PasswordListSortMode, Preferences, UsernameFallbackMode};
