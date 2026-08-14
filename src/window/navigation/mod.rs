mod pages;
mod restore;
mod state;

pub use self::pages::show_log_page;
#[cfg(target_os = "linux")]
pub use self::restore::restore_window_for_current_page;
pub use self::state::WindowNavigationState;
