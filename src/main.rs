#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(all(target_os = "linux", feature = "setup"))]
mod setup;

mod composition;
#[cfg(target_os = "linux")]
mod search_provider;
mod translation_help;
mod updater;
mod window;

use adw::gio::{resources_register_include, ApplicationFlags};
use adw::gtk::{gdk::Display, glib::ExitCode};
use adw::{Application, ApplicationWindow};
use keycord_entries::model::OpenPassFile;
#[cfg(feature = "passkey")]
use keycord_passkey::ui::OpenPasskeyRequest;
use keycord_runtime::capabilities::handle_unsupported_host_command_invocation;
use keycord_runtime::hardening::apply_process_hardening;
use keycord_runtime::log_error;
use keycord_shell::application::{
    run_application, AboutDialogConfig, ActivationCallbacks, ActivationRequest,
    ApplicationCallbacks, ApplicationConfig, StartupHook,
};
use keycord_shell::navigation::APP_WINDOW_TITLE;
use keycord_shell::object_data::{set_cloned_data, set_string_data, take_data, take_string_data};
#[cfg(all(target_os = "linux", feature = "setup"))]
use keycord_shell::theme::install_color_scheme_tracking;
use std::ffi::OsString;

const APP_ID: &str = env!("APP_ID");
const RESOURCE_ID: &str = env!("RESOURCE_ID");
const ISSUE_URL: &str = concat!(env!("CARGO_PKG_REPOSITORY"), "/issues");
#[cfg(feature = "passkey")]
const OPEN_PASSKEY_REQUEST_KEY: &str = "open-passkey-request";
struct MainWindowCommand {
    query: Option<String>,
    pass_file: Option<OpenPassFile>,
}

struct AfterWindowPresent {
    #[cfg(feature = "passkey")]
    passkey_request: Option<OpenPasskeyRequest>,
}

fn main() -> ExitCode {
    let args = std::env::args_os().collect::<Vec<_>>();
    if handle_unsupported_host_command_invocation(&args) {
        return 126.into();
    }
    if let Some(code) = updater::handle_special_command(&args) {
        return code;
    }
    #[cfg(target_os = "linux")]
    if search_provider::is_search_provider_command(&args) {
        return search_provider::run();
    }

    composition::localization::init();
    if let Err(error) = apply_process_hardening() {
        log_error(format!("Failed to apply process hardening: {error}"));
    }

    run_application(application_config(), application_callbacks())
}

fn application_config() -> ApplicationConfig {
    ApplicationConfig {
        application_id: APP_ID,
        icon_resource_path: RESOURCE_ID,
        flags: ApplicationFlags::HANDLES_OPEN | ApplicationFlags::HANDLES_COMMAND_LINE,
        about: AboutDialogConfig {
            application_title: APP_WINDOW_TITLE,
            application_icon: APP_ID,
            version: env!("CARGO_PKG_VERSION"),
            authors: env!("CARGO_PKG_AUTHORS"),
            homepage: env!("CARGO_PKG_HOMEPAGE"),
            repository_url: env!("CARGO_PKG_REPOSITORY"),
            issue_url: ISSUE_URL,
            translation_url: translation_help::TRANSLATION_URL,
            translator_credits: "Translated by Nick.",
        },
    }
}

fn application_callbacks() -> ApplicationCallbacks<MainWindowCommand, AfterWindowPresent> {
    ApplicationCallbacks {
        register_resources: Box::new(|| {
            resources_register_include!("compiled.gresource").map_err(|error| error.to_string())
        }),
        display_ready: Box::new(handle_display_ready),
        startup_hooks: vec![StartupHook::new(
            "Failed to prepare managed private-key storage.",
            || composition::backend::prepare_startup().map(|_| ()),
        )],
        register_actions: Box::new(|app| {
            updater::register_app_actions(app);
            translation_help::register_app_actions(app);
        }),
        handle_open: Box::new(handle_open_files),
        handle_command_line: Box::new(dispatch_command_line_args),
        shutdown_hooks: vec![
            Box::new(|_| composition::backend::clear_runtime_secret_state()),
            Box::new(updater::shutdown),
        ],
        about_comments: Box::new(composition::about::comments),
        activation: ActivationCallbacks {
            take_request: Box::new(take_activation_request),
            dispatch_existing_window: Box::new(dispatch_existing_window),
            create_window: Box::new(create_window),
            new_window_presented: Box::new(new_window_presented),
            after_present: Box::new(after_window_presented),
        },
    }
}

fn handle_display_ready(_display: &Display) {
    #[cfg(all(target_os = "linux", feature = "setup"))]
    install_color_scheme_tracking(_display);
}

fn handle_open_files(_app: &Application, _files: &[adw::gio::File], _hint: &str) {
    #[cfg(feature = "passkey")]
    set_cloned_data(
        _app,
        OPEN_PASSKEY_REQUEST_KEY,
        keycord_passkey::ui::open_request_from_files(_files),
    );
}

fn take_activation_request(
    app: &Application,
) -> ActivationRequest<MainWindowCommand, AfterWindowPresent> {
    ActivationRequest {
        window_command: MainWindowCommand {
            query: take_string_data(app, "query"),
            pass_file: take_data(app, "open-pass-file"),
        },
        after_present: AfterWindowPresent {
            #[cfg(feature = "passkey")]
            passkey_request: take_data(app, OPEN_PASSKEY_REQUEST_KEY),
        },
    }
}

fn dispatch_existing_window(window: &ApplicationWindow, command: MainWindowCommand) {
    window::dispatch_main_window_command(window, command.query, command.pass_file);
}

fn create_window(
    app: &Application,
    command: MainWindowCommand,
) -> Result<ApplicationWindow, String> {
    window::create_main_window(app, command.query, command.pass_file)
}

fn new_window_presented(app: &Application, window: &ApplicationWindow) {
    updater::after_window_presented(app, window);
    translation_help::show_notification_once(app);
}

fn after_window_presented(_window: &ApplicationWindow, _pending: AfterWindowPresent) {
    #[cfg(feature = "passkey")]
    if let Some(passkey_request) = _pending.passkey_request {
        composition::passkey_dialog::present_open_passkey_request(_window, passkey_request);
    }
}

fn dispatch_command_line_args(app: &Application, args: &[OsString]) {
    if let Some(pass_file) = keycord_entries::launch::command_line_open_entry(args) {
        set_cloned_data(app, "open-pass-file", pass_file);
        return;
    }

    #[cfg(feature = "passkey")]
    if let Some(passkey_request) = keycord_passkey::ui::command_line_request(args) {
        set_cloned_data(app, OPEN_PASSKEY_REQUEST_KEY, passkey_request);
        return;
    }

    if let Some(query) = command_line_query(args) {
        set_string_data(app, "query", query);
    }
}

fn command_line_query(args: &[OsString]) -> Option<String> {
    if args.len() <= 1 || args.get(1).is_some_and(|arg| arg == "--open-entry") {
        return None;
    }

    args[1..]
        .join(&OsString::from(" "))
        .into_string()
        .ok()
        .filter(|query| !query.is_empty())
}

#[cfg(test)]
mod tests {
    use super::command_line_query;
    use std::ffi::OsString;

    #[test]
    fn free_form_arguments_become_a_query() {
        let args = vec![
            OsString::from("keycord"),
            OsString::from("find"),
            OsString::from("otp"),
            OsString::from("and"),
            OsString::from("user"),
            OsString::from("alice"),
        ];

        assert_eq!(
            command_line_query(&args),
            Some("find otp and user alice".to_string())
        );
        assert!(keycord_entries::launch::command_line_open_entry(&args).is_none());
    }
}
