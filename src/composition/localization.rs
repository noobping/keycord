#[cfg(target_os = "linux")]
use keycord_runtime::i18n::I18nConfig;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::sync::Once;

#[cfg(target_os = "linux")]
const DOMAIN: &str = env!("GETTEXT_DOMAIN");
#[cfg(target_os = "linux")]
const DEFAULT_LOCALEDIR: &str = env!("LOCALEDIR");
#[cfg(target_os = "linux")]
const AVAILABLE_LOCALES: &str = env!("AVAILABLE_LOCALES");

pub fn init() {
    #[cfg(target_os = "linux")]
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

#[cfg(target_os = "linux")]
fn runtime_locale_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(locale_dir) = std::env::var("KEYCORD_LOCALEDIR") {
        candidates.push(PathBuf::from(locale_dir));
    }

    candidates.push(PathBuf::from("/app/share/locale"));
    candidates.push(PathBuf::from("/usr/local/share/locale"));
    candidates.push(PathBuf::from("/usr/share/locale"));
    candidates.push(PathBuf::from(DEFAULT_LOCALEDIR));

    if let Some(data_dir) = dirs_next::data_dir() {
        candidates.push(data_dir.join("locale"));
    }

    candidates
}
