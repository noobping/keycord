use super::types::{is_url_field_key, DynamicFieldRow, DynamicFieldTemplate, StructuredPassLine};
use super::url::add_open_url_suffix;
use adw::gtk::{Box as GtkBox, Widget};
use adw::{prelude::*, ActionRow, EntryRow, PasswordEntryRow, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_shell::clipboard::add_copy_suffix;
use keycord_shell::ui::clear_box_children;
use std::cell::RefCell;
use std::rc::Rc;

pub fn rebuild_dynamic_fields_from_lines(
    box_widget: &GtkBox,
    overlay: &ToastOverlay,
    templates_state: &Rc<RefCell<Vec<StructuredPassLine>>>,
    rows_state: &Rc<RefCell<Vec<DynamicFieldRow>>>,
    structured_lines: &[(StructuredPassLine, Option<String>)],
) {
    clear_box_children(box_widget);
    templates_state.borrow_mut().clear();
    rows_state.borrow_mut().clear();

    let mut rows = Vec::new();
    let mut templates = Vec::new();
    let mut has_visible_rows = false;

    for (line, value) in structured_lines.iter().cloned() {
        match line {
            StructuredPassLine::Field(template) => {
                let row =
                    dynamic_field_row(&template, value.as_deref().unwrap_or_default(), overlay);
                box_widget.append(&row.widget());
                rows.push(row);
                templates.push(StructuredPassLine::Field(template));
                has_visible_rows = true;
            }
            StructuredPassLine::Username(template) => {
                templates.push(StructuredPassLine::Username(template));
            }
            StructuredPassLine::Otp(template) => {
                templates.push(StructuredPassLine::Otp(template));
            }
            StructuredPassLine::Passkey(template) => {
                let credential = &template.credential;
                let subtitle = format!("{} — {}", credential.username, credential.rp_id);
                let row = ActionRow::builder()
                    .title(gettext("Passkey"))
                    .subtitle(subtitle)
                    .build();
                apply_field_row_style(&row);
                box_widget.append(&row);
                templates.push(StructuredPassLine::Passkey(template));
                has_visible_rows = true;
            }
            StructuredPassLine::Preserved(line) => {
                templates.push(StructuredPassLine::Preserved(line));
            }
        }
    }

    box_widget.set_visible(has_visible_rows);
    *templates_state.borrow_mut() = templates;
    *rows_state.borrow_mut() = rows;
}

pub fn dynamic_field_row(
    template: &DynamicFieldTemplate,
    value: &str,
    overlay: &ToastOverlay,
) -> DynamicFieldRow {
    if template.sensitive {
        let row = PasswordEntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        DynamicFieldRow::Secret(row)
    } else {
        let row = EntryRow::new();
        row.set_title(&template.title);
        row.set_text(value);
        apply_field_row_style(&row);
        let row_clone = row.clone();
        add_copy_suffix(&row, move || row_clone.text().to_string(), overlay);
        if is_url_field_key(&template.raw_key) {
            let row_clone = row.clone();
            add_open_url_suffix(&row, move || row_clone.text().to_string(), overlay);
        }
        DynamicFieldRow::Plain(row)
    }
}

fn apply_field_row_style<W: IsA<Widget>>(widget: &W) {
    widget.set_margin_start(15);
    widget.set_margin_end(15);
    widget.set_margin_bottom(6);
}
