mod compose;
mod parse;
#[cfg(feature = "ui")]
mod row_ui;
#[cfg(test)]
mod tests;
mod types;
#[cfg(feature = "ui")]
mod url;

pub use self::compose::structured_pass_contents_from_values;
pub use self::compose::{
    apply_pass_file_template_contents, clean_pass_file_contents,
    new_pass_file_contents_from_template, pass_file_has_missing_template_fields,
};
#[cfg(feature = "ui")]
pub use self::compose::{
    structured_pass_contents, sync_username_row, sync_username_row_from_parsed_lines,
};
pub use self::parse::structured_username_value;
pub use self::parse::{
    canonical_search_field_key, is_passkey_storage_line, pass_file_has_otp,
    pass_file_has_passkey_storage_field, searchable_pass_fields, SearchablePassField,
};
pub use self::parse::{parse_structured_pass_lines, structured_otp_line};
#[cfg(feature = "ui")]
pub use self::row_ui::{dynamic_field_row, rebuild_dynamic_fields_from_lines};
#[cfg(feature = "ui")]
pub use self::types::DynamicFieldRow;
pub use self::types::UsernameFieldTemplate;
pub use self::types::{DynamicFieldTemplate, OtpFieldTemplate, StructuredPassLine};
#[cfg(feature = "ui")]
pub use self::url::uri_to_open;
