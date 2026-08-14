use crate::composition::entries_ui::collect_all_password_items_with_options;
use keycord_entries::model::CollectItemsOptions;
use keycord_lifecycle::search_provider::{
    SearchProviderConfig, SearchProviderEntry, SearchProviderPorts,
};
use keycord_stores::labels::shortened_store_labels;
use std::collections::HashMap;
use std::ffi::OsString;

fn list_entries() -> Vec<SearchProviderEntry> {
    collect_all_password_items_with_options(CollectItemsOptions::default())
        .into_iter()
        .map(|entry| {
            let label = entry.label();
            SearchProviderEntry {
                store_path: entry.store_path,
                label,
                basename: entry.basename,
            }
        })
        .collect()
}

fn store_labels() -> HashMap<String, String> {
    let stores = keycord_preferences::Preferences::new().store_roots();
    let labels = shortened_store_labels(&stores);
    stores.into_iter().zip(labels).collect()
}

fn config() -> SearchProviderConfig {
    SearchProviderConfig {
        app_id: env!("APP_ID"),
        bus_name: env!("SEARCH_PROVIDER_BUS_NAME"),
        object_path: env!("SEARCH_PROVIDER_OBJECT_PATH"),
    }
}

fn ports() -> SearchProviderPorts {
    SearchProviderPorts {
        list_entries,
        store_labels,
    }
}

pub(crate) fn is_search_provider_command(args: &[OsString]) -> bool {
    keycord_lifecycle::search_provider::is_search_provider_command(args)
}

pub(crate) fn run() -> adw::glib::ExitCode {
    keycord_lifecycle::search_provider::run(config(), ports())
}
