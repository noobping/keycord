use adw::gio::prelude::*;
#[cfg(target_os = "linux")]
use adw::gio::AppInfo;
use adw::gio::{Notification, SimpleAction};
use adw::Application;
use keycord_preferences::Preferences;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::uri::launch_default_uri;

#[cfg(target_os = "linux")]
const APP_ID: &str = env!("APP_ID");
pub(crate) const TRANSLATION_URL: &str = "https://hosted.weblate.org/projects/keycord/";

const NOTIFICATION_ID: &str = "translation-help";
const OPEN_TRANSLATION_ACTION: &str = "open-translation-project";
const OPEN_TRANSLATION_DETAILED_ACTION: &str = "app.open-translation-project";

pub(crate) fn register_app_actions(app: &Application) {
    let action = SimpleAction::new(OPEN_TRANSLATION_ACTION, None);
    action.connect_activate(|_, _| {
        launch_default_uri(TRANSLATION_URL, |result| {
            if let Err(error) = result {
                log_error(format!(
                    "Failed to open the translation project.\nURL: {TRANSLATION_URL}\nerror: {error}"
                ));
            }
        });
    });
    app.add_action(&action);
}

pub(crate) fn show_notification_once(app: &Application) {
    let preferences = Preferences::new();
    if !should_send_notification(
        preferences.translation_help_notification_sent(),
        desktop_notification_delivery_available(),
    ) {
        return;
    }

    if let Err(error) = preferences.set_translation_help_notification_sent(true) {
        log_error(format!(
            "Failed to save the translation-help notification state: {error}"
        ));
        return;
    }

    let title = gettext("Help translate Keycord");
    let body = gettext(
        "Translate Keycord into a new language or help improve an existing translation on Weblate.",
    );
    let button = gettext("Open Weblate");
    let notification = Notification::new(&title);
    notification.set_body(Some(&body));
    notification.add_button(&button, OPEN_TRANSLATION_DETAILED_ACTION);
    app.send_notification(Some(NOTIFICATION_ID), &notification);
}

const fn should_send_notification(already_sent: bool, delivery_available: bool) -> bool {
    !already_sent && delivery_available
}

fn desktop_notification_delivery_available() -> bool {
    #[cfg(target_os = "linux")]
    {
        // GLib requires the application desktop file to be installed before it can deliver a
        // GNotification. Do not consume the one-time preference for an uninstalled cargo build.
        let desktop_id = format!("{APP_ID}.desktop");
        return AppInfo::all()
            .iter()
            .any(|app| app.id().as_deref() == Some(desktop_id.as_str()));
    }

    #[cfg(not(target_os = "linux"))]
    true
}

#[cfg(test)]
mod tests {
    use super::{should_send_notification, OPEN_TRANSLATION_DETAILED_ACTION, TRANSLATION_URL};

    #[test]
    fn translation_destination_is_the_keycord_weblate_project() {
        assert_eq!(
            TRANSLATION_URL,
            "https://hosted.weblate.org/projects/keycord/"
        );
    }

    #[test]
    fn notification_button_targets_an_application_action() {
        assert!(OPEN_TRANSLATION_DETAILED_ACTION.starts_with("app."));
    }

    #[test]
    fn notification_requires_available_delivery_and_an_unsent_request() {
        assert!(should_send_notification(false, true));
        assert!(!should_send_notification(true, true));
        assert!(!should_send_notification(false, false));
        assert!(!should_send_notification(true, false));
    }
}
