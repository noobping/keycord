pub(crate) fn install_config() -> keycord_lifecycle::setup::InstallConfig {
    keycord_lifecycle::setup::InstallConfig {
        product_name: env!("CARGO_PKG_NAME"),
        display_name: env!("DISPLAY_NAME"),
        product_description: env!("CARGO_PKG_DESCRIPTION"),
        app_id: env!("APP_ID"),
        gettext_domain: env!("GETTEXT_DOMAIN"),
        locale_dir: env!("LOCALEDIR"),
        available_locales: env!("AVAILABLE_LOCALES"),
        localized_desktop_entry: include_str!(concat!(env!("OUT_DIR"), "/keycord.desktop")),
        resource_id: env!("RESOURCE_ID"),
        search_provider_bus_name: env!("SEARCH_PROVIDER_BUS_NAME"),
        search_provider_object_path: env!("SEARCH_PROVIDER_OBJECT_PATH"),
        passkey_mime: cfg!(feature = "passkey").then_some(
            keycord_lifecycle::desktop::PasskeyMimeConfig {
                mime_types: keycord_passkey::PASSKEY_MIME_TYPES,
                package: keycord_passkey::PASSKEY_MIME_PACKAGE,
            },
        ),
        initialize_i18n: crate::composition::localization::init,
        register_resources,
    }
}

fn register_resources() -> Result<(), String> {
    adw::gio::resources_register_include!("compiled.gresource")
        .map(|_| ())
        .map_err(|error| format!("Failed to register resources for Linux update install: {error}"))
}

pub fn append_local_install_row(
    list: &adw::gtk::ListBox,
    overlay: &adw::ToastOverlay,
    on_changed: impl Fn() + 'static,
) -> Option<adw::ActionRow> {
    keycord_lifecycle::setup::append_local_install_row(list, overlay, install_config(), on_changed)
}

pub fn sync_local_install_row(row: Option<&adw::ActionRow>) {
    keycord_lifecycle::setup::sync_local_install_row(row, &install_config());
}
