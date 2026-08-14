//! Generic application-window widget bundle and shell-owned navigation policy.

use adw::glib::{self, Propagation};
use adw::gtk::{gdk, Builder, Button, EventControllerKey, MenuButton, TextView};
use adw::prelude::*;
use adw::{
    ActionRow, ApplicationWindow, NavigationPage, NavigationView, ToastOverlay, WindowTitle,
};

use crate::navigation::PagePresentation;
use crate::ui::{required_builder_object, visible_navigation_page_is, widget_contains_focus};
use std::rc::Rc;

pub type WindowFocusCallback = Rc<dyn Fn() -> bool>;

#[derive(Clone)]
pub struct ShellWindowWidgets {
    pub window: ApplicationWindow,
    pub navigation: NavigationView,
    pub back: Button,
    pub title: WindowTitle,
    pub primary_menu: MenuButton,
    pub overlay: ToastOverlay,
    pub log_page: NavigationPage,
    pub log_view: TextView,
    pub log_tool_row: ActionRow,
    pub copy_logs_tool_row: ActionRow,
    pub copy_logs_button: Button,
}

impl ShellWindowWidgets {
    pub fn load(builder: &Builder) -> Result<Self, String> {
        Ok(Self {
            window: required_builder_object(builder, "main_window")?,
            navigation: required_builder_object(builder, "navigation_view")?,
            back: required_builder_object(builder, "back_button")?,
            title: required_builder_object(builder, "window_title")?,
            primary_menu: required_builder_object(builder, "primary_menu_button")?,
            overlay: required_builder_object(builder, "toast_overlay")?,
            log_page: required_builder_object(builder, "log_page")?,
            log_view: required_builder_object(builder, "log_view")?,
            log_tool_row: required_builder_object(builder, "tools_logs_row")?,
            copy_logs_tool_row: required_builder_object(builder, "tools_copy_logs_row")?,
            copy_logs_button: required_builder_object(builder, "tools_copy_logs_button")?,
        })
    }

    pub fn focus_log_target(&self) -> bool {
        self.log_view.grab_focus()
    }

    /// Apply Shell-owned log page availability.
    pub fn set_logging_available(&self, available: bool) {
        self.log_page.set_visible(available);
    }

    pub fn focus_first_visible_page_target(&self, navigation: &NavigationView) -> Option<bool> {
        visible_navigation_page_is(navigation, &self.log_page).then(|| self.focus_log_target())
    }

    pub fn visible_page_contains_focus(&self, navigation: &NavigationView) -> Option<bool> {
        visible_navigation_page_is(navigation, &self.log_page)
            .then(|| widget_contains_focus(&self.log_page.clone().upcast()))
    }

    /// Connects generic window focus behavior to the composed page policy.
    pub fn connect_page_focus_navigation(
        &self,
        visible_page_contains_focus: WindowFocusCallback,
        focus_first_visible_page_target: WindowFocusCallback,
    ) {
        let focus_target = focus_first_visible_page_target.clone();
        let controller = EventControllerKey::new();
        controller.set_propagation_phase(adw::gtk::PropagationPhase::Capture);
        controller.connect_key_pressed(move |controller, key, _, _| {
            if !matches!(key, gdk::Key::Down | gdk::Key::KP_Down) {
                return Propagation::Proceed;
            }
            let Some(window) = controller
                .widget()
                .and_then(|widget| widget.downcast::<ApplicationWindow>().ok())
            else {
                return Propagation::Proceed;
            };
            let Some(focus) = adw::gtk::prelude::RootExt::focus(&window) else {
                return Propagation::Proceed;
            };
            if focus.ancestor(adw::HeaderBar::static_type()).is_none() {
                return Propagation::Proceed;
            }
            if focus_target() {
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        });
        self.window.add_controller(controller);

        let contains_focus = visible_page_contains_focus.clone();
        let focus_target = focus_first_visible_page_target.clone();
        self.navigation
            .connect_notify_local(Some("visible-page"), move |_, _| {
                schedule_page_focus(contains_focus.clone(), focus_target.clone());
            });
    }

    pub fn schedule_page_focus(
        &self,
        visible_page_contains_focus: WindowFocusCallback,
        focus_first_visible_page_target: WindowFocusCallback,
    ) {
        schedule_page_focus(visible_page_contains_focus, focus_first_visible_page_target);
    }
}

fn schedule_page_focus(
    visible_page_contains_focus: WindowFocusCallback,
    focus_first_visible_page_target: WindowFocusCallback,
) {
    glib::idle_add_local_once(move || {
        if !visible_page_contains_focus() {
            let _ = focus_first_visible_page_target();
        }
    });
}

pub fn log_page_presentation() -> PagePresentation {
    PagePresentation::secondary("Logs", "Details", false)
}

pub fn configure_shell_shortcuts(app: &adw::Application) {
    app.set_accels_for_action("win.back", &["Escape"]);
    app.set_accels_for_action("win.go-home", &["Home"]);
    app.set_accels_for_action("app.shortcuts", &["<primary>question"]);
    #[cfg(feature = "logging")]
    app.set_accels_for_action("win.open-log", &["F12"]);
}
