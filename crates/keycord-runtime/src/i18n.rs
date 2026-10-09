//! Gettext integration configured by the composing application.

use std::path::PathBuf;

/// Runtime translation settings supplied by the application package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct I18nConfig {
    domain: String,
    default_locale_dir: PathBuf,
    available_locales: Vec<String>,
    locale_dir_candidates: Vec<PathBuf>,
}

impl I18nConfig {
    pub fn new<I, S>(
        domain: impl Into<String>,
        default_locale_dir: impl Into<PathBuf>,
        available_locales: I,
    ) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            domain: domain.into(),
            default_locale_dir: default_locale_dir.into(),
            available_locales: available_locales.into_iter().map(Into::into).collect(),
            locale_dir_candidates: Vec::new(),
        }
    }

    pub fn with_locale_dir_candidates<I, P>(mut self, candidates: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.locale_dir_candidates = candidates.into_iter().map(Into::into).collect();
        self
    }
}

/// Initializes process-wide translations from application-owned configuration.
pub fn initialize(config: I18nConfig) -> Result<(), String> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    return native::initialize(config);

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = config;
        Ok(())
    }
}

/// Translates a message after [`initialize`] has configured gettext.
pub fn gettext(message: &str) -> String {
    if message.is_empty() {
        return String::new();
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    return native::gettext(message);

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    message.to_string()
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
mod native {
    use super::I18nConfig;
    use libc::{c_char, LC_ALL};
    use std::ffi::{CStr, CString};
    use std::path::{Path, PathBuf};
    use std::sync::OnceLock;

    static INITIALIZATION: OnceLock<Result<(), String>> = OnceLock::new();

    #[cfg_attr(target_os = "windows", link(name = "intl"))]
    unsafe extern "C" {
        #[cfg(target_os = "linux")]
        #[link_name = "bindtextdomain"]
        fn bindtextdomain_raw(domainname: *const c_char, dirname: *const c_char) -> *mut c_char;
        #[cfg(target_os = "windows")]
        #[link_name = "libintl_wbindtextdomain"]
        fn wbindtextdomain_raw(domainname: *const c_char, dirname: *const u16) -> *mut u16;
        #[cfg_attr(target_os = "linux", link_name = "bind_textdomain_codeset")]
        #[cfg_attr(target_os = "windows", link_name = "libintl_bind_textdomain_codeset")]
        fn bind_textdomain_codeset_raw(
            domainname: *const c_char,
            codeset: *const c_char,
        ) -> *mut c_char;
        #[cfg_attr(target_os = "linux", link_name = "textdomain")]
        #[cfg_attr(target_os = "windows", link_name = "libintl_textdomain")]
        fn textdomain_raw(domainname: *const c_char) -> *mut c_char;
        #[cfg_attr(target_os = "linux", link_name = "gettext")]
        #[cfg_attr(target_os = "windows", link_name = "libintl_gettext")]
        fn gettext_raw(message: *const c_char) -> *mut c_char;
        #[cfg_attr(target_os = "windows", link_name = "libintl_setlocale")]
        fn setlocale(category: libc::c_int, locale: *const c_char) -> *mut c_char;
    }

    pub(super) fn initialize(config: I18nConfig) -> Result<(), String> {
        INITIALIZATION
            .get_or_init(|| initialize_once(config))
            .clone()
    }

    fn initialize_once(config: I18nConfig) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        super::windows::configure_language()?;

        let empty_locale = CString::new("").map_err(|error| error.to_string())?;
        let domain = CString::new(config.domain.as_str())
            .map_err(|_| "The gettext domain contains an embedded NUL byte.".to_string())?;
        let locale_dir = preferred_locale_dir(&config);
        #[cfg(target_os = "linux")]
        let locale_dir = CString::new(locale_dir.to_string_lossy().as_bytes())
            .map_err(|_| "The gettext locale path contains an embedded NUL byte.".to_string())?;
        #[cfg(target_os = "windows")]
        let locale_dir = {
            use std::os::windows::ffi::OsStrExt;
            let mut path = locale_dir.as_os_str().encode_wide().collect::<Vec<_>>();
            if path.contains(&0) {
                return Err("The gettext locale path contains an embedded NUL byte.".to_string());
            }
            path.push(0);
            path
        };
        let codeset = CString::new("UTF-8").map_err(|error| error.to_string())?;

        unsafe {
            setlocale(LC_ALL, empty_locale.as_ptr());
            #[cfg(target_os = "linux")]
            bindtextdomain_raw(domain.as_ptr(), locale_dir.as_ptr());
            #[cfg(target_os = "windows")]
            wbindtextdomain_raw(domain.as_ptr(), locale_dir.as_ptr());
            bind_textdomain_codeset_raw(domain.as_ptr(), codeset.as_ptr());
            textdomain_raw(domain.as_ptr());
        }
        Ok(())
    }

    fn preferred_locale_dir(config: &I18nConfig) -> PathBuf {
        config
            .locale_dir_candidates
            .iter()
            .find(|candidate| has_domain_catalog(candidate, config))
            .cloned()
            .or_else(|| {
                config
                    .locale_dir_candidates
                    .iter()
                    .find(|candidate| candidate.exists())
                    .cloned()
            })
            .unwrap_or_else(|| config.default_locale_dir.clone())
    }

    fn has_domain_catalog(locale_dir: &Path, config: &I18nConfig) -> bool {
        config.available_locales.iter().any(|locale| {
            locale_dir
                .join(locale)
                .join("LC_MESSAGES")
                .join(format!("{}.mo", config.domain))
                .exists()
        })
    }

    pub(super) fn gettext(message: &str) -> String {
        if !matches!(INITIALIZATION.get(), Some(Ok(()))) {
            return message.to_string();
        }

        let Ok(message) = CString::new(message) else {
            return message.to_string();
        };

        unsafe {
            let translated = gettext_raw(message.as_ptr());
            if translated.is_null() {
                return message.to_string_lossy().into_owned();
            }

            CStr::from_ptr(translated).to_string_lossy().into_owned()
        }
    }
}

#[cfg(target_os = "windows")]
mod windows;

#[cfg(test)]
mod tests {
    use super::{gettext, I18nConfig};

    #[test]
    fn config_owns_values_supplied_by_the_application() {
        let domain = String::from("keycord-test");
        let locale = String::from("nl");
        let config = I18nConfig::new(domain, "/tmp/keycord-locale", [locale])
            .with_locale_dir_candidates(["/app/share/locale"]);

        assert_eq!(config.domain, "keycord-test");
        assert_eq!(config.available_locales, ["nl"]);
        assert_eq!(config.locale_dir_candidates.len(), 1);
    }

    #[test]
    fn empty_messages_stay_empty_without_initialization() {
        assert_eq!(gettext(""), "");
    }
}
