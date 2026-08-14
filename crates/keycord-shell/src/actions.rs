use adw::gio::SimpleAction;
use adw::gtk::Widget;
use adw::prelude::*;
use adw::ApplicationWindow;

pub fn register_window_action(
    window: &ApplicationWindow,
    name: &str,
    activate: impl Fn() + 'static,
) {
    let action = SimpleAction::new(name, None);
    action.connect_activate(move |_, _| activate());
    window.add_action(&action);
}

pub fn activate_widget_action(widget: &impl IsA<Widget>, action_name: &str) {
    let _ = widget.activate_action(action_name, None);
}

/// Enables or disables a registered simple window action.
///
/// Returns `false` when the action is absent or is not a [`SimpleAction`].
pub fn set_window_action_enabled(window: &ApplicationWindow, name: &str, enabled: bool) -> bool {
    let Some(action) = window.lookup_action(name) else {
        return false;
    };
    let Ok(action) = action.downcast::<SimpleAction>() else {
        return false;
    };
    action.set_enabled(enabled);
    true
}
