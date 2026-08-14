use super::{
    append_loading_rows, collect_loaded_entry_requests, EntryToolsState, FieldValueRequest,
    FIELD_VALUES_EMPTY_SUBTITLE, FIELD_VALUES_EMPTY_TITLE, FIELD_VALUES_FIELDS_SUBTITLE,
    FIELD_VALUES_FILTER_EMPTY_SUBTITLE, FIELD_VALUES_FILTER_EMPTY_TITLE,
    FIELD_VALUES_LOADING_SUBTITLE, FIELD_VALUES_LOADING_TITLE, FIELD_VALUES_TITLE,
    FIELD_VALUES_VALUES_SUBTITLE, VALUE_VALUES_EMPTY_SUBTITLE, VALUE_VALUES_EMPTY_TITLE,
    VALUE_VALUES_FILTER_EMPTY_SUBTITLE, VALUE_VALUES_FILTER_EMPTY_TITLE,
};
use crate::file::searchable_pass_fields;
pub(super) use crate::tools::{
    field_value_catalog_from_entries, format_exact_field_query, matching_items_subtitle,
    unique_values_subtitle, FieldValueCatalog,
};
#[cfg(test)]
pub(super) use crate::tools::{FieldCatalogEntry, ValueCatalogEntry};
use crate::ui::opened::clear_opened_pass_file;
use adw::prelude::*;
use keycord_shell::background::spawn_result_task;
use keycord_shell::navigation::{show_secondary_page_chrome, HasWindowChrome};
use keycord_shell::ui::next_ui_generation as next_generation;
use keycord_shell::ui::{
    append_action_row_with_button, append_info_row, clear_list_box, pop_navigation_to_root,
    reveal_navigation_page,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Default)]
pub(super) struct FieldValueBrowserState {
    pub(super) generation: Cell<u64>,
    pub(super) in_flight: Cell<bool>,
    pub(super) tool_busy: Cell<bool>,
    pub(super) source_generation: Cell<Option<u64>>,
    pub(super) catalog: RefCell<Option<FieldValueCatalog>>,
    pub(super) selected_field: RefCell<Option<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FieldValueCatalogBatch {
    generation: u64,
    catalog: FieldValueCatalog,
}

impl EntryToolsState {
    pub(super) fn prepare_field_values_browser(&self) {
        if self.advanced_search_tools_are_busy() {
            return;
        }

        self.invalidate_stale_tool_cache();
        self.reset_field_values_view();

        let source_generation = self.current_password_list_generation();
        if self.field_values_cache_is_current(source_generation) {
            self.field_browser.browser.in_flight.set(false);
            self.render_field_list();
            self.render_value_list();
            self.show_field_values_browser_page();
            return;
        }

        self.set_field_values_tool_busy(true);
        let requests = collect_loaded_entry_requests(&self.root_list);
        let generation = next_generation(self.field_browser.browser.generation.get());
        self.field_browser.browser.generation.set(generation);
        self.field_browser
            .browser
            .source_generation
            .set(source_generation);
        self.field_browser.browser.in_flight.set(true);
        *self.field_browser.browser.catalog.borrow_mut() = None;
        self.render_field_list();
        self.render_value_list();
        self.show_field_values_browser_page();

        self.unlock_tool_keys_if_needed(
            requests,
            Rc::new({
                let state = self.clone();
                move |requests| state.open_field_values_browser_with_requests(generation, requests)
            }),
            Rc::new({
                let state = self.clone();
                move || state.handle_field_catalog_disconnect(generation)
            }),
        );
    }

    fn show_field_values_browser_page(&self) {
        self.close_select_dialog();
        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(
            &chrome,
            FIELD_VALUES_TITLE,
            FIELD_VALUES_FIELDS_SUBTITLE,
            false,
        );
        chrome.find.set_visible(true);
        reveal_navigation_page(&self.navigation.nav, &self.field_browser.field_page);
    }

    fn open_field_values_browser_with_requests(
        &self,
        generation: u64,
        requests: Vec<FieldValueRequest>,
    ) {
        if generation != self.field_browser.browser.generation.get() {
            return;
        }

        if requests.is_empty() {
            self.apply_field_catalog_batch(FieldValueCatalogBatch {
                generation,
                catalog: FieldValueCatalog::default(),
            });
            return;
        }

        let state_for_result = self.clone();
        let state_for_disconnect = self.clone();
        let read_entry = self.ports.backend.read_entry.clone();
        spawn_result_task(
            move || build_field_value_catalog_batch(generation, requests, &read_entry),
            move |batch| state_for_result.apply_field_catalog_batch(batch),
            move || state_for_disconnect.handle_field_catalog_disconnect(generation),
        );
    }

    fn open_value_values_browser(&self, field_key: &str) {
        let field_changed = self
            .field_browser
            .browser
            .selected_field
            .borrow()
            .as_deref()
            != Some(field_key);
        *self.field_browser.browser.selected_field.borrow_mut() = Some(field_key.to_string());
        if field_changed && !self.field_browser.value_search_entry.text().is_empty() {
            self.field_browser.value_search_entry.set_text("");
        }
        self.render_value_list();

        let chrome = self.navigation.window_chrome();
        show_secondary_page_chrome(
            &chrome,
            FIELD_VALUES_TITLE,
            FIELD_VALUES_VALUES_SUBTITLE,
            false,
        );
        chrome.find.set_visible(true);
        reveal_navigation_page(&self.navigation.nav, &self.field_browser.value_page);
    }

    fn apply_field_catalog_batch(&self, batch: FieldValueCatalogBatch) {
        if batch.generation != self.field_browser.browser.generation.get() {
            return;
        }

        self.field_browser.browser.in_flight.set(false);
        self.set_field_values_tool_busy(false);
        *self.field_browser.browser.catalog.borrow_mut() = Some(batch.catalog);
        self.render_field_list();
        self.render_value_list();
    }

    fn handle_field_catalog_disconnect(&self, generation: u64) {
        if generation != self.field_browser.browser.generation.get() {
            return;
        }

        self.field_browser.browser.in_flight.set(false);
        self.set_field_values_tool_busy(false);
        self.render_field_list();
        self.render_value_list();
    }

    pub(super) fn render_field_list(&self) {
        clear_list_box(&self.field_browser.field_list);

        if self.field_browser.browser.in_flight.get() {
            append_loading_rows(
                &self.field_browser.field_list,
                FIELD_VALUES_LOADING_TITLE,
                FIELD_VALUES_LOADING_SUBTITLE,
            );
            return;
        }

        let Some(catalog) = self.field_browser.browser.catalog.borrow().clone() else {
            append_info_row(
                &self.field_browser.field_list,
                FIELD_VALUES_EMPTY_TITLE,
                FIELD_VALUES_EMPTY_SUBTITLE,
            );
            return;
        };

        let query = self.field_browser.field_search_entry.text();
        let query = query.as_str().trim().to_lowercase();
        let fields = catalog
            .fields
            .iter()
            .filter(|field| query.is_empty() || field.key.contains(&query))
            .cloned()
            .collect::<Vec<_>>();

        if fields.is_empty() {
            append_info_row(
                &self.field_browser.field_list,
                if query.is_empty() {
                    FIELD_VALUES_EMPTY_TITLE
                } else {
                    FIELD_VALUES_FILTER_EMPTY_TITLE
                },
                if query.is_empty() {
                    FIELD_VALUES_EMPTY_SUBTITLE
                } else {
                    FIELD_VALUES_FILTER_EMPTY_SUBTITLE
                },
            );
            return;
        }

        for field in fields {
            let subtitle = unique_values_subtitle(field.unique_value_count);
            let state = self.clone();
            let field_key = field.key.clone();
            append_action_row_with_button(
                &self.field_browser.field_list,
                &field.key,
                &subtitle,
                "go-next-symbolic",
                move || state.open_value_values_browser(&field_key),
            );
        }
    }

    pub(super) fn render_value_list(&self) {
        clear_list_box(&self.field_browser.value_list);

        let Some(selected_field) = self.field_browser.browser.selected_field.borrow().clone()
        else {
            append_info_row(
                &self.field_browser.value_list,
                VALUE_VALUES_EMPTY_TITLE,
                VALUE_VALUES_EMPTY_SUBTITLE,
            );
            return;
        };

        let Some(catalog) = self.field_browser.browser.catalog.borrow().clone() else {
            if self.field_browser.browser.in_flight.get() {
                append_loading_rows(
                    &self.field_browser.value_list,
                    FIELD_VALUES_LOADING_TITLE,
                    FIELD_VALUES_LOADING_SUBTITLE,
                );
            } else {
                append_info_row(
                    &self.field_browser.value_list,
                    VALUE_VALUES_EMPTY_TITLE,
                    VALUE_VALUES_EMPTY_SUBTITLE,
                );
            }
            return;
        };

        let query = self.field_browser.value_search_entry.text();
        let query = query.as_str().trim().to_lowercase();
        let values = catalog
            .values_by_field
            .get(&selected_field)
            .into_iter()
            .flatten()
            .filter(|value| query.is_empty() || value.normalized_value.contains(&query))
            .cloned()
            .collect::<Vec<_>>();

        if values.is_empty() {
            append_info_row(
                &self.field_browser.value_list,
                if query.is_empty() {
                    VALUE_VALUES_EMPTY_TITLE
                } else {
                    VALUE_VALUES_FILTER_EMPTY_TITLE
                },
                if query.is_empty() {
                    VALUE_VALUES_EMPTY_SUBTITLE
                } else {
                    VALUE_VALUES_FILTER_EMPTY_SUBTITLE
                },
            );
            return;
        }

        for value in values {
            let subtitle = matching_items_subtitle(value.match_count);
            let state = self.clone();
            let field = selected_field.clone();
            let display_value = value.display_value.clone();
            append_action_row_with_button(
                &self.field_browser.value_list,
                &value.display_value,
                &subtitle,
                "go-next-symbolic",
                move || state.apply_root_search(&format_exact_field_query(&field, &display_value)),
            );
        }
    }

    fn apply_root_search(&self, query: &str) {
        self.reset_field_values_view();
        pop_navigation_to_root(&self.navigation.nav);
        clear_opened_pass_file(&self.navigation.nav);

        let chrome = self.navigation.window_chrome();
        (self.ports.show_root_page)(&chrome);

        self.root_search_entry.set_visible(true);
        self.root_search_entry.set_text(query);
        self.root_list.invalidate_filter();
        self.root_search_entry.grab_focus();
    }

    pub(super) fn reset_field_values_view(&self) {
        *self.field_browser.browser.selected_field.borrow_mut() = None;
        self.field_browser.field_search_entry.set_visible(false);
        self.field_browser.value_search_entry.set_visible(false);

        if !self.field_browser.field_search_entry.text().is_empty() {
            self.field_browser.field_search_entry.set_text("");
        }
        if !self.field_browser.value_search_entry.text().is_empty() {
            self.field_browser.value_search_entry.set_text("");
        }
    }

    pub(super) fn clear_field_values_cache(&self) {
        self.field_browser
            .browser
            .generation
            .set(next_generation(self.field_browser.browser.generation.get()));
        self.field_browser.browser.in_flight.set(false);
        self.field_browser.browser.source_generation.set(None);
        *self.field_browser.browser.catalog.borrow_mut() = None;
        self.set_field_values_tool_busy(false);
        self.reset_field_values_view();
    }
}

fn build_field_value_catalog_batch(
    generation: u64,
    requests: Vec<FieldValueRequest>,
    read_entry: &super::ReadToolEntry,
) -> FieldValueCatalogBatch {
    let indexed_entries = requests
        .into_iter()
        .filter_map(|request| {
            read_entry(request.root.clone(), request.label.clone())
                .ok()
                .map(|contents| searchable_pass_fields(&contents))
        })
        .collect::<Vec<_>>();

    FieldValueCatalogBatch {
        generation,
        catalog: field_value_catalog_from_entries(indexed_entries),
    }
}
