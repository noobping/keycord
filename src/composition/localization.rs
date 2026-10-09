#[cfg(any(target_os = "linux", target_os = "windows"))]
use keycord_runtime::i18n::I18nConfig;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::path::PathBuf;
#[cfg(any(target_os = "linux", target_os = "windows"))]
use std::sync::Once;

#[cfg(any(target_os = "linux", target_os = "windows"))]
const DOMAIN: &str = env!("GETTEXT_DOMAIN");
#[cfg(any(target_os = "linux", target_os = "windows"))]
const DEFAULT_LOCALEDIR: &str = env!("LOCALEDIR");
#[cfg(any(target_os = "linux", target_os = "windows"))]
const AVAILABLE_LOCALES: &str = env!("AVAILABLE_LOCALES");

pub fn init() {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        static INIT: Once = Once::new();
        INIT.call_once(|| {
            let config = I18nConfig::new(
                DOMAIN,
                DEFAULT_LOCALEDIR,
                AVAILABLE_LOCALES
                    .split(':')
                    .filter(|locale| !locale.is_empty()),
            )
            .with_locale_dir_candidates(runtime_locale_dir_candidates());
            if let Err(error) = keycord_runtime::i18n::initialize(config) {
                keycord_runtime::log_error(format!("Failed to initialize translations: {error}"));
            }
        });
    }
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn runtime_locale_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(locale_dir) = std::env::var("KEYCORD_LOCALEDIR") {
        candidates.push(PathBuf::from(locale_dir));
    }

    #[cfg(target_os = "windows")]
    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.push(directory.join("share").join("locale"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        candidates.push(PathBuf::from("/app/share/locale"));
        candidates.push(PathBuf::from("/usr/local/share/locale"));
        candidates.push(PathBuf::from("/usr/share/locale"));
    }
    candidates.push(PathBuf::from(DEFAULT_LOCALEDIR));

    if let Some(data_dir) = dirs_next::data_dir() {
        candidates.push(data_dir.join("locale"));
    }

    candidates
}

#[cfg(all(test, any(target_os = "linux", target_os = "windows")))]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Requires a display and an en_US.UTF-8 locale on Linux"]
    fn translated_rust_and_builder_text() {
        const CHILD: &str = "KEYCORD_TRANSLATION_TEST_EXPECTED";
        if let Ok(expected) = std::env::var(CHILD) {
            init();
            assert_eq!(keycord_runtime::i18n::gettext("Close"), expected);
            assert_eq!(keycord_runtime::i18n::gettext(""), "");
            assert_eq!(
                keycord_runtime::i18n::gettext("Untranslated test message"),
                "Untranslated test message"
            );
            adw::gtk::init().expect("GTK initialization");
            let builder = adw::gtk::Builder::from_string(&format!(
                r#"<interface domain="{DOMAIN}">
                    <object class="GtkButton" id="close">
                        <property name="label" translatable="yes">Close</property>
                    </object>
                </interface>"#
            ));
            let button: adw::gtk::Button = builder.object("close").unwrap();
            use adw::gtk::prelude::ButtonExt;
            assert_eq!(button.label().as_deref(), Some(expected.as_str()));
            return;
        }

        // Exercise portable catalogs in a path containing spaces and Unicode,
        // independently of the build machine's locale installation.
        let directory = std::env::temp_dir().join(format!(
            "keycord translation test 日本語 {}",
            std::process::id()
        ));
        for language in ["nl", "de"] {
            let relative = PathBuf::from(language)
                .join("LC_MESSAGES")
                .join(format!("{DOMAIN}.mo"));
            let destination = directory.join(&relative);
            std::fs::create_dir_all(destination.parent().unwrap()).unwrap();
            std::fs::copy(PathBuf::from(DEFAULT_LOCALEDIR).join(relative), destination).unwrap();
        }
        for (language, expected) in [
            ("nl_NL", "Sluiten"),
            ("de_DE", "Schließen"),
            ("en", "Close"),
            ("zz", "Close"),
        ] {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "composition::localization::tests::translated_rust_and_builder_text",
                    "--nocapture",
                    "--ignored",
                ])
                .env(CHILD, expected)
                .env("KEYCORD_LOCALEDIR", &directory)
                .env("LANGUAGE", language)
                .env(
                    "LC_ALL",
                    if cfg!(windows) {
                        "en_US"
                    } else {
                        "en_US.UTF-8"
                    },
                )
                .status()
                .unwrap();
            assert!(status.success(), "Translation check failed for {language}");
        }
        std::fs::remove_dir_all(directory).unwrap();
    }
}
