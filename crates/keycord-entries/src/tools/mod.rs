//! Entry analysis and export engines used by the application UI.

mod export;
mod field_values;
mod weak_passwords;

pub use export::{export_passwords_to_csv_with, unique_store_roots, EXPORT_FILE_NAME};
pub use field_values::{
    field_value_catalog_from_entries, format_exact_field_query, matching_items_subtitle,
    unique_values_subtitle, FieldCatalogEntry, FieldValueCatalog, ValueCatalogEntry,
};
pub use weak_passwords::{weak_password_findings_with, WeakPasswordFinding};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryRequest {
    pub root: String,
    pub label: String,
}
