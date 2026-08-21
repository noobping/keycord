use adw::gio::prelude::*;
use adw::gio::{Notification, SimpleAction};
use adw::Application;
use keycord_preferences::Preferences;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::uri::launch_default_uri;

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
    if preferences.translation_help_notification_shown() {
        return;
    }

    if let Err(error) = preferences.set_translation_help_notification_shown(true) {
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

#[cfg(test)]
mod tests {
    use super::{OPEN_TRANSLATION_DETAILED_ACTION, TRANSLATION_URL};

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
}
