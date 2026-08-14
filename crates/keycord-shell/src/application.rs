//! Generic GTK/libadwaita application bootstrap and lifecycle wiring.

use crate::object_data::{cloned_data, set_cloned_data};
use adw::gio::{self, ApplicationFlags, SimpleAction};
use adw::gtk::{gdk::Display, glib::ExitCode, Builder, IconTheme, License};
use adw::prelude::*;
use adw::{Application, ApplicationWindow, ShortcutsDialog};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::fs;
#[cfg(target_os = "windows")]
use std::hash::{Hash, Hasher};
#[cfg(any(target_os = "windows", test))]
use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use winsafe::{self as w, co};

const MAIN_WINDOW_ACTIVATING_KEY: &str = "main-window-activating";

/// Static metadata used to build the application and its standard dialogs.
pub struct ApplicationConfig {
    pub application_id: &'static str,
    pub icon_resource_path: &'static str,
    pub flags: ApplicationFlags,
    pub about: AboutDialogConfig,
}

#[derive(Clone, Copy)]
pub struct AboutDialogConfig {
    pub application_title: &'static str,
    pub application_icon: &'static str,
    pub version: &'static str,
    pub authors: &'static str,
    pub homepage: &'static str,
    pub repository_url: &'static str,
    pub issue_url: &'static str,
    pub translation_url: &'static str,
    pub translator_credits: &'static str,
}

pub type ResourceRegistrationCallback = Box<dyn Fn() -> Result<(), String>>;
pub type DisplayReadyCallback = Box<dyn Fn(&Display)>;
pub type ApplicationCallback = Box<dyn Fn(&Application)>;
pub type OpenCallback = Box<dyn Fn(&Application, &[gio::File], &str)>;
pub type CommandLineCallback = Box<dyn Fn(&Application, &[OsString])>;
pub type AboutCommentsCallback = Box<dyn Fn(&str) -> String>;

pub struct StartupHook {
    failure_summary: &'static str,
    callback: Box<dyn Fn() -> Result<(), String>>,
}

impl StartupHook {
    pub fn new(
        failure_summary: &'static str,
        callback: impl Fn() -> Result<(), String> + 'static,
    ) -> Self {
        Self {
            failure_summary,
            callback: Box::new(callback),
        }
    }
}

/// Values consumed by one activation, split around window presentation.
pub struct ActivationRequest<WindowCommand, AfterPresent> {
    pub window_command: WindowCommand,
    pub after_present: AfterPresent,
}

pub type TakeActivationCallback<WindowCommand, AfterPresent> =
    Box<dyn Fn(&Application) -> ActivationRequest<WindowCommand, AfterPresent>>;
pub type ExistingWindowCallback<WindowCommand> = Box<dyn Fn(&ApplicationWindow, WindowCommand)>;
pub type CreateWindowCallback<WindowCommand> =
    Box<dyn Fn(&Application, WindowCommand) -> Result<ApplicationWindow, String>>;
pub type NewWindowPresentedCallback = Box<dyn Fn(&Application, &ApplicationWindow)>;
pub type AfterPresentCallback<AfterPresent> = Box<dyn Fn(&ApplicationWindow, AfterPresent)>;

/// Callback ports for the domain-specific parts of window activation.
pub struct ActivationCallbacks<WindowCommand, AfterPresent> {
    pub take_request: TakeActivationCallback<WindowCommand, AfterPresent>,
    pub dispatch_existing_window: ExistingWindowCallback<WindowCommand>,
    pub create_window: CreateWindowCallback<WindowCommand>,
    pub new_window_presented: NewWindowPresentedCallback,
    pub after_present: AfterPresentCallback<AfterPresent>,
}

/// Callback ports invoked by the generic application bootstrap.
pub struct ApplicationCallbacks<WindowCommand, AfterPresent> {
    pub register_resources: ResourceRegistrationCallback,
    pub display_ready: DisplayReadyCallback,
    pub startup_hooks: Vec<StartupHook>,
    pub register_actions: ApplicationCallback,
    pub handle_open: OpenCallback,
    pub handle_command_line: CommandLineCallback,
    pub shutdown_hooks: Vec<ApplicationCallback>,
    pub about_comments: AboutCommentsCallback,
    pub activation: ActivationCallbacks<WindowCommand, AfterPresent>,
}

/// Initializes the GUI runtime, wires the application lifecycle, and runs it.
pub fn run_application<WindowCommand: 'static, AfterPresent: 'static>(
    config: ApplicationConfig,
    callbacks: ApplicationCallbacks<WindowCommand, AfterPresent>,
) -> ExitCode {
    let ApplicationConfig {
        application_id,
        icon_resource_path,
        flags,
        about,
    } = config;
    let ApplicationCallbacks {
        register_resources,
        display_ready,
        startup_hooks,
        register_actions,
        handle_open,
        handle_command_line,
        shutdown_hooks,
        about_comments,
        activation,
    } = callbacks;

    #[cfg(target_os = "windows")]
    configure_windows_runtime_environment(application_id);

    if let Err(error) = register_resources() {
        return startup_error(
            about.application_title,
            "Failed to register resources.",
            &error,
        );
    }
    if let Err(error) = adw::init() {
        return startup_error(
            about.application_title,
            "Failed to initialize libadwaita.",
            &error.to_string(),
        );
    }

    let Some(display) = Display::default() else {
        return startup_error(
            about.application_title,
            "No display available.",
            "missing display",
        );
    };
    display_ready(&display);
    IconTheme::for_display(&display).add_resource_path(icon_resource_path);

    for hook in startup_hooks {
        if let Err(error) = (hook.callback)() {
            return startup_error(about.application_title, hook.failure_summary, &error);
        }
    }

    let app = Application::builder()
        .application_id(application_id)
        .flags(flags)
        .build();
    app.set_accels_for_action("app.about", &["F1"]);
    register_actions(&app);
    register_standard_actions(&app, about, about_comments);

    app.connect_open(move |app, files, hint| {
        handle_open(app, files, hint);
        app.activate();
    });

    app.connect_command_line(move |app, command_line| {
        let args = command_line.arguments();
        handle_command_line(app, &args);
        app.activate();
        0.into()
    });

    app.connect_shutdown(move |app| {
        for hook in &shutdown_hooks {
            hook(app);
        }
    });

    let ActivationCallbacks {
        take_request,
        dispatch_existing_window,
        create_window,
        new_window_presented,
        after_present,
    } = activation;
    app.connect_activate(move |app| {
        let Some(_activation_guard) = MainWindowActivationGuard::acquire(app) else {
            return;
        };
        let ActivationRequest {
            window_command,
            after_present: pending_after_present,
        } = take_request(app);

        if let Some(window) = existing_main_window(app) {
            dispatch_existing_window(&window, window_command);
            window.present();
            after_present(&window, pending_after_present);
            return;
        }

        match create_window(app, window_command) {
            Ok(window) => {
                window.present();
                new_window_presented(app, &window);
                after_present(&window, pending_after_present);
            }
            Err(error) => {
                report_startup_error(
                    about.application_title,
                    "Failed to build the main window.",
                    &error,
                );
                app.quit();
            }
        }
    });

    app.run()
}

struct MainWindowActivationGuard {
    app: Application,
}

impl MainWindowActivationGuard {
    fn acquire(app: &Application) -> Option<Self> {
        if cloned_data::<_, bool>(app, MAIN_WINDOW_ACTIVATING_KEY).unwrap_or(false) {
            return None;
        }

        set_cloned_data(app, MAIN_WINDOW_ACTIVATING_KEY, true);
        Some(Self { app: app.clone() })
    }
}

impl Drop for MainWindowActivationGuard {
    fn drop(&mut self) {
        set_cloned_data(&self.app, MAIN_WINDOW_ACTIVATING_KEY, false);
    }
}

fn existing_main_window(app: &Application) -> Option<ApplicationWindow> {
    app.active_window()
        .and_then(|window| window.downcast::<ApplicationWindow>().ok())
        .or_else(|| {
            app.windows()
                .into_iter()
                .find_map(|window| window.downcast::<ApplicationWindow>().ok())
        })
}

fn register_standard_actions(
    app: &Application,
    config: AboutDialogConfig,
    about_comments: AboutCommentsCallback,
) {
    let about_action = SimpleAction::new("about", None);
    let app_for_about = app.clone();
    about_action.connect_activate(move |_, _| {
        let about = build_about_dialog(config, &about_comments);
        about.present(app_for_about.active_window().as_ref());
    });
    app.add_action(&about_action);

    let shortcuts_action = SimpleAction::new("shortcuts", None);
    let app_for_shortcuts = app.clone();
    shortcuts_action.connect_activate(move |_, _| match build_shortcuts_dialog() {
        Ok(shortcuts) => shortcuts.present(app_for_shortcuts.active_window().as_ref()),
        Err(error) => log_error(format!(
            "Failed to build the shortcuts dialog.\nerror: {error}"
        )),
    });
    app.add_action(&shortcuts_action);
}

fn build_shortcuts_dialog() -> Result<ShortcutsDialog, String> {
    let builder = Builder::from_string(crate::SHORTCUTS_UI);
    builder
        .object("shortcuts_dialog")
        .ok_or_else(|| "Failed to build shortcuts dialog.".to_string())
}

fn build_about_dialog(
    config: AboutDialogConfig,
    about_comments: &AboutCommentsCallback,
) -> adw::AboutDialog {
    let application_name = gettext(config.application_title);
    let authors: Vec<_> = config.authors.split(':').collect();
    let developer_name = authors
        .first()
        .map(|author| author_display_name(author.trim()))
        .unwrap_or(application_name.as_str());
    let about = adw::AboutDialog::builder()
        .application_name(&application_name)
        .application_icon(config.application_icon)
        .version(config.version)
        .developer_name(developer_name)
        .developers(&authors[..])
        .comments(about_comments(&application_name))
        .translator_credits(gettext(config.translator_credits))
        .license_type(License::Gpl30Only)
        .website(config.homepage)
        .issue_url(config.issue_url)
        .support_url(config.issue_url)
        .build();
    about.add_link(&gettext("Repository"), config.repository_url);
    about.add_link(&gettext("Translate"), config.translation_url);
    about
}

fn author_display_name(author: &str) -> &str {
    author.split_once(" <").map_or(author, |(name, _)| name)
}

fn report_startup_error(title: &str, summary: &str, error: &str) {
    let detail = format!("{summary}\nerror: {error}");
    log_error(detail.clone());
    eprintln!("{title}: {detail}");
    #[cfg(target_os = "windows")]
    show_windows_startup_error_dialog(title, &detail);
}

fn startup_error(title: &str, summary: &str, error: &str) -> ExitCode {
    report_startup_error(title, summary, error);
    1.into()
}

#[cfg(target_os = "windows")]
fn show_windows_startup_error_dialog(title: &str, body: &str) {
    let _ = w::HWND::GetDesktopWindow().MessageBox(body, title, co::MB::OK | co::MB::ICONERROR);
}

#[cfg(target_os = "windows")]
fn configure_windows_runtime_environment(application_id: &str) {
    let Some(root) = windows_runtime_root() else {
        return;
    };

    set_windows_env_path_if_exists("GTK_EXE_PREFIX", &root);
    set_windows_env_path_if_exists("GTK_DATA_PREFIX", &root);

    let share = root.join("share");
    prepend_windows_env_path("XDG_DATA_DIRS", &share);
    prepend_windows_env_path("XDG_CONFIG_DIRS", &root.join("etc"));

    let schemas = share.join("glib-2.0").join("schemas");
    if schemas.join("gschemas.compiled").is_file() {
        set_windows_env_path_if_exists("GSETTINGS_SCHEMA_DIR", &schemas);
    }

    let pixbuf_root = root.join("lib").join("gdk-pixbuf-2.0").join("2.10.0");
    let pixbuf_modules = pixbuf_root.join("loaders");
    let pixbuf_cache =
        rewritten_windows_pixbuf_cache(application_id, &root, &pixbuf_root, &pixbuf_modules)
            .unwrap_or_else(|| pixbuf_root.join("loaders.cache"));
    if pixbuf_cache.is_file() {
        set_windows_env_path_if_exists("GDK_PIXBUF_MODULE_FILE", &pixbuf_cache);
    }
    prepend_windows_env_path("GDK_PIXBUF_MODULEDIR", &pixbuf_modules);
}

#[cfg(target_os = "windows")]
fn windows_runtime_root() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
}

#[cfg(target_os = "windows")]
fn set_windows_env_path_if_exists(name: &str, path: &Path) {
    if path.exists() {
        std::env::set_var(name, path);
    }
}

#[cfg(target_os = "windows")]
fn prepend_windows_env_path(name: &str, path: &Path) {
    if !path.exists() {
        return;
    }

    let mut paths = std::env::var_os(name)
        .map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .unwrap_or_default();
    if !paths.iter().any(|existing| existing == path) {
        paths.insert(0, path.to_path_buf());
    }
    if let Ok(joined) = std::env::join_paths(paths) {
        std::env::set_var(name, joined);
    }
}

#[cfg(target_os = "windows")]
fn rewritten_windows_pixbuf_cache(
    application_id: &str,
    runtime_root: &Path,
    pixbuf_root: &Path,
    pixbuf_modules: &Path,
) -> Option<PathBuf> {
    let source_cache = pixbuf_root.join("loaders.cache");
    if !source_cache.is_file() || !pixbuf_modules.is_dir() {
        return None;
    }

    let source = fs::read_to_string(&source_cache).ok()?;
    let rewritten = rewrite_pixbuf_loader_cache(&source, pixbuf_modules);
    let output = windows_pixbuf_cache_output_path(application_id, runtime_root)?;
    let parent = output.parent()?;
    fs::create_dir_all(parent).ok()?;

    let should_write = fs::read_to_string(&output)
        .map(|existing| existing != rewritten)
        .unwrap_or(true);
    if should_write && fs::write(&output, rewritten).is_err() {
        return None;
    }

    Some(output)
}

#[cfg(target_os = "windows")]
fn windows_pixbuf_cache_output_path(application_id: &str, runtime_root: &Path) -> Option<PathBuf> {
    let base = dirs_next::cache_dir().unwrap_or_else(std::env::temp_dir);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    runtime_root.hash(&mut hasher);
    let hash = hasher.finish();
    Some(
        base.join(application_id)
            .join("gdk-pixbuf")
            .join(format!("loaders-{hash:016x}.cache")),
    )
}

#[cfg(any(target_os = "windows", test))]
fn rewrite_pixbuf_loader_cache(source: &str, pixbuf_modules: &Path) -> String {
    let loader_dir = pixbuf_modules.display().to_string().replace('\\', "/");
    source
        .lines()
        .map(|line| {
            if line.starts_with("# LoaderDir = ") {
                return format!("# LoaderDir = {loader_dir}");
            }

            let Some(loader_name) = quoted_pixbuf_loader_name(line) else {
                return line.to_string();
            };
            let rewritten = pixbuf_modules
                .join(loader_name)
                .display()
                .to_string()
                .replace('\\', "/");
            format!("\"{rewritten}\"")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(any(target_os = "windows", test))]
fn quoted_pixbuf_loader_name(line: &str) -> Option<&str> {
    let inner = line.strip_prefix('"')?.strip_suffix('"')?;
    let name = inner.rsplit(['/', '\\']).next()?;
    let ext = name.rsplit('.').next()?;
    if name.is_empty() || !ext.eq_ignore_ascii_case("dll") {
        return None;
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::{author_display_name, quoted_pixbuf_loader_name, rewrite_pixbuf_loader_cache};
    use std::path::Path;

    #[test]
    fn author_display_name_removes_email_address() {
        assert_eq!(
            author_display_name("A Person <person@example.com>"),
            "A Person"
        );
        assert_eq!(author_display_name("A Person"), "A Person");
    }

    #[test]
    fn pixbuf_loader_cache_rewrite_uses_runtime_loader_dir() {
        let source = concat!(
            "# LoaderDir = C:/tools/msys64/mingw64/lib/gdk-pixbuf-2.0/2.10.0/loaders\n",
            "\"C:/tools/msys64/mingw64/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.dll\"\n",
            "\"svg\" 6 \"gdk-pixbuf\" \"Scalable Vector Graphics\" \"LGPL\""
        );
        let modules = Path::new(
            r"C:\Users\nick\AppData\Local\Programs\Keycord\lib\gdk-pixbuf-2.0\2.10.0\loaders",
        );

        let rewritten = rewrite_pixbuf_loader_cache(source, modules);

        assert!(rewritten.contains(
            "# LoaderDir = C:/Users/nick/AppData/Local/Programs/Keycord/lib/gdk-pixbuf-2.0/2.10.0/loaders"
        ));
        assert!(rewritten.contains(
            "\"C:/Users/nick/AppData/Local/Programs/Keycord/lib/gdk-pixbuf-2.0/2.10.0/loaders/libpixbufloader-svg.dll\""
        ));
        assert!(
            rewritten.contains("\"svg\" 6 \"gdk-pixbuf\" \"Scalable Vector Graphics\" \"LGPL\"")
        );
    }

    #[test]
    fn pixbuf_loader_name_only_matches_loader_path_lines() {
        assert_eq!(
            quoted_pixbuf_loader_name("\"C:/msys64/libpixbufloader-svg.dll\""),
            Some("libpixbufloader-svg.dll")
        );
        assert_eq!(quoted_pixbuf_loader_name("\"svg\" 6 \"gdk-pixbuf\""), None);
        assert_eq!(quoted_pixbuf_loader_name("# LoaderDir = C:/tmp"), None);
    }
}
