use crate::file::SearchablePassField;
use keycord_runtime::i18n::gettext;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldValueCatalog {
    pub fields: Vec<FieldCatalogEntry>,
    pub values_by_field: BTreeMap<String, Vec<ValueCatalogEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldCatalogEntry {
    pub key: String,
    pub unique_value_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueCatalogEntry {
    pub display_value: String,
    pub normalized_value: String,
    pub match_count: usize,
}

pub fn field_value_catalog_from_entries(
    indexed_entries: impl IntoIterator<Item = Vec<SearchablePassField>>,
) -> FieldValueCatalog {
    #[derive(Default)]
    struct ValueAccumulator {
        display_value: String,
        match_count: usize,
    }

    let mut values_by_field: BTreeMap<String, BTreeMap<String, ValueAccumulator>> = BTreeMap::new();
    for entry_fields in indexed_entries {
        let mut entry_values: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for field in entry_fields {
            if field.key == "otpauth" {
                continue;
            }
            entry_values
                .entry(field.key)
                .or_default()
                .entry(field.normalized_value)
                .or_insert(field.value);
        }
        for (field_key, entry_unique_values) in entry_values {
            let field_values = values_by_field.entry(field_key).or_default();
            for (normalized_value, display_value) in entry_unique_values {
                let value = field_values.entry(normalized_value).or_default();
                if value.display_value.is_empty() {
                    value.display_value = display_value;
                }
                value.match_count += 1;
            }
        }
    }

    let fields = values_by_field
        .iter()
        .map(|(key, values)| FieldCatalogEntry {
            key: key.clone(),
            unique_value_count: values.len(),
        })
        .collect();
    let values_by_field = values_by_field
        .into_iter()
        .map(|(key, values)| {
            let values = values
                .into_iter()
                .map(|(normalized_value, value)| ValueCatalogEntry {
                    display_value: value.display_value,
                    normalized_value,
                    match_count: value.match_count,
                })
                .collect();
            (key, values)
        })
        .collect();

    FieldValueCatalog {
        fields,
        values_by_field,
    }
}

pub fn format_exact_field_query(field: &str, value: &str) -> String {
    format!(
        "find \"{}\" is \"{}\"",
        escape_quoted_search_component(field),
        escape_quoted_search_component(value)
    )
}

fn escape_quoted_search_component(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn unique_values_subtitle(count: usize) -> String {
    let template = if count == 1 {
        gettext("{count} unique value")
    } else {
        gettext("{count} unique values")
    };
    template.replace("{count}", &count.to_string())
}

pub fn matching_items_subtitle(count: usize) -> String {
    let template = if count == 1 {
        gettext("{count} matching item")
    } else {
        gettext("{count} matching items")
    };
    template.replace("{count}", &count.to_string())
}
