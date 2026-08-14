pub const PRODUCT_NAME: &str = "keycord";
pub const PRODUCT_DESCRIPTION: &str = "Browse and edit password stores";
pub const GETTEXT_DOMAIN: &str = PRODUCT_NAME;
pub const RESOURCE_ID: &str = "/io/github/noobping/keycord";
pub const RELEASE_APP_ID: &str = "io.github.noobping.keycord";
pub const DEVELOPMENT_APP_ID: &str = "io.github.noobping.keycord-beta";

const DESKTOP_FILE_TEMPLATE: &str = include_str!("../data/keycord.desktop.in");
const SEARCH_PROVIDER_FILE_TEMPLATE: &str = include_str!("../data/keycord-search-provider.ini.in");
const SEARCH_PROVIDER_SERVICE_TEMPLATE: &str =
    include_str!("../data/keycord-search-provider.service.in");

/// Passkey-owned MIME metadata supplied by the application composition layer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PasskeyMimeConfig<'a> {
    /// Semicolon-delimited MIME types advertised by the desktop entry.
    pub mime_types: &'a str,
    /// Shared MIME-info XML installed for local application installs.
    pub package: &'a str,
}

pub const fn app_id(debug: bool, flatpak: bool) -> &'static str {
    if debug && !flatpak {
        DEVELOPMENT_APP_ID
    } else {
        RELEASE_APP_ID
    }
}

pub fn search_provider_bus_name(app_id: &str) -> String {
    format!("{}.SearchProvider", app_id.replace('-', "_"))
}

pub fn search_provider_object_path(bus_name: &str) -> String {
    format!("/{}", bus_name.replace('.', "/"))
}

pub fn passkey_fields(passkey_mime: Option<PasskeyMimeConfig<'_>>) -> (&'static str, String) {
    passkey_mime.map_or_else(
        || ("", String::new()),
        |config| (" %f", format!("MimeType={}\n", config.mime_types)),
    )
}

pub fn desktop_file(
    app_id: &str,
    executable: &str,
    display_name: &str,
    comment: &str,
    passkey_mime: Option<PasskeyMimeConfig<'_>>,
) -> String {
    let (open_argument, mime_types) = passkey_fields(passkey_mime);
    DESKTOP_FILE_TEMPLATE
        .replace("@DISPLAY_NAME@", display_name)
        .replace("@COMMENT@", comment)
        .replace("@EXECUTABLE@", executable)
        .replace("@OPEN_ARGUMENT@", open_argument)
        .replace("@APP_ID@", app_id)
        .replace("@MIME_TYPES@", mime_types.trim_end())
}

pub fn search_provider_file(app_id: &str, bus_name: &str, object_path: &str) -> String {
    SEARCH_PROVIDER_FILE_TEMPLATE
        .replace("@APP_ID@", app_id)
        .replace("@BUS_NAME@", bus_name)
        .replace("@OBJECT_PATH@", object_path)
}

pub fn search_provider_service_file(bus_name: &str, executable: &str) -> String {
    SEARCH_PROVIDER_SERVICE_TEMPLATE
        .replace("@BUS_NAME@", bus_name)
        .replace("@EXECUTABLE@", executable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_identifiers_preserve_release_and_development_names() {
        assert_eq!(app_id(false, false), RELEASE_APP_ID);
        assert_eq!(app_id(true, true), RELEASE_APP_ID);
        assert_eq!(app_id(true, false), DEVELOPMENT_APP_ID);
        let bus = search_provider_bus_name(DEVELOPMENT_APP_ID);
        assert_eq!(bus, "io.github.noobping.keycord_beta.SearchProvider");
        assert_eq!(
            search_provider_object_path(&bus),
            "/io/github/noobping/keycord_beta/SearchProvider"
        );
    }

    #[test]
    fn generated_search_provider_files_keep_dbus_contract() {
        let bus = search_provider_bus_name(RELEASE_APP_ID);
        let object = search_provider_object_path(&bus);
        assert!(search_provider_file(RELEASE_APP_ID, &bus, &object).contains("Version=2\n"));
        assert!(search_provider_service_file(&bus, PRODUCT_NAME)
            .contains("Exec=keycord --search-provider\n"));
    }

    #[test]
    fn install_asset_templates_render_without_unresolved_tokens() {
        let bus = search_provider_bus_name(RELEASE_APP_ID);
        let object = search_provider_object_path(&bus);
        let rendered = [
            desktop_file(
                RELEASE_APP_ID,
                PRODUCT_NAME,
                "Keycord",
                PRODUCT_DESCRIPTION,
                None,
            ),
            search_provider_file(RELEASE_APP_ID, &bus, &object),
            search_provider_service_file(&bus, PRODUCT_NAME),
        ];

        for asset in rendered {
            assert!(!asset.contains('@'), "unresolved install-asset token");
        }
    }

    #[test]
    fn passkey_builds_advertise_request_handlers() {
        let config = PasskeyMimeConfig {
            mime_types: "application/vnd.example.passkey+json;",
            package: "<mime-info />",
        };
        let (open_argument, mime_types) = passkey_fields(Some(config));

        assert_eq!(open_argument, " %f");
        assert_eq!(
            mime_types,
            "MimeType=application/vnd.example.passkey+json;\n"
        );
    }

    #[test]
    fn builds_without_passkeys_do_not_advertise_request_handlers() {
        let (open_argument, mime_types) = passkey_fields(None);

        assert!(open_argument.is_empty());
        assert!(mime_types.is_empty());
    }
}
