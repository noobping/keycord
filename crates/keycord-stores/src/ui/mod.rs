//! Store-specific GTK presentation.

mod dialogs;
pub mod management;
pub mod ports;
pub mod recipient_page;
mod widgets;

pub use dialogs::build_progress_dialog;
pub use widgets::{configure_store_shortcuts, store_import_page_presentation, StoresWindowWidgets};
