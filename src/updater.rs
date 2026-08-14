use adw::{Application, ApplicationWindow, ToastOverlay};
use std::ffi::OsString;

fn configure_install() {
    #[cfg(all(target_os = "linux", feature = "setup"))]
    keycord_lifecycle::updater::configure_install(crate::setup::install_config());
}

pub fn register_app_actions(app: &Application) {
    configure_install();
    keycord_lifecycle::updater::register_app_actions(app);
}

pub fn register_window(
    app: &Application,
    window: &ApplicationWindow,
    overlay: &ToastOverlay,
    has_unsaved_changes: keycord_lifecycle::updater::DirtyProbe,
) {
    configure_install();
    keycord_lifecycle::updater::register_window(app, window, overlay, has_unsaved_changes);
}

pub fn after_window_presented(app: &Application, window: &ApplicationWindow) {
    configure_install();
    keycord_lifecycle::updater::after_window_presented(app, window);
}

pub fn shutdown(app: &Application) {
    configure_install();
    keycord_lifecycle::updater::shutdown(app);
}

pub fn handle_special_command(args: &[OsString]) -> Option<adw::glib::ExitCode> {
    configure_install();
    keycord_lifecycle::updater::handle_special_command(args)
}
