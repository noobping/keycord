//! Root adapters for subject-specific navigation actions.

use super::state::WindowNavigationState;
use keycord_runtime::capabilities::supports_logging_features;
use keycord_shell::navigation::HasWindowChrome;
use keycord_shell::{log_page_presentation, navigation::show_navigation_page};

pub fn show_log_page(state: &WindowNavigationState) {
    if !supports_logging_features() {
        return;
    }

    let chrome = state.window_chrome();
    show_navigation_page(
        &state.nav,
        &state.log_page,
        &chrome,
        &log_page_presentation(),
    );
}
