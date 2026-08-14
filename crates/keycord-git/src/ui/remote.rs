//! Git remote editor presentation.

use adw::gtk::{Align, Box as GtkBox, Label, Orientation};
use adw::prelude::*;
use adw::{ApplicationWindow, Dialog, EntryRow, PreferencesGroup, PreferencesPage};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::ui::dialog_content_shell;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[doc(hidden)]
pub fn next_available_remote_name(base: &str, existing_names: &[String]) -> String {
    if !existing_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(base))
    {
        return base.to_string();
    }

    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !existing_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        suffix += 1;
    }
}

#[doc(hidden)]
pub fn suggested_remote_name_from_url(url: &str, existing_names: &[String]) -> Option<String> {
    (!url.trim().is_empty()).then(|| next_available_remote_name("origin", existing_names))
}

#[doc(hidden)]
pub fn remote_name_exists(name: &str, existing_names: &[String]) -> bool {
    let name = name.trim();
    existing_names
        .iter()
        .any(|existing_name| existing_name.eq_ignore_ascii_case(name))
}

#[doc(hidden)]
pub fn remote_url_exists(url: &str, existing_urls: &[String]) -> bool {
    let url = url.trim();
    existing_urls.iter().any(|existing_url| existing_url == url)
}

#[doc(hidden)]
pub fn remote_dialog_error_message(
    name: &str,
    url: &str,
    existing_names: &[String],
    existing_urls: &[String],
) -> Option<&'static str> {
    if name.trim().is_empty() {
        return Some("Enter a remote name.");
    }
    if remote_name_exists(name, existing_names) {
        return Some("That remote name already exists.");
    }
    if url.trim().is_empty() {
        return Some("Enter a remote URL.");
    }
    if remote_url_exists(url, existing_urls) {
        return Some("That remote URL already exists.");
    }

    None
}

#[doc(hidden)]
pub fn next_autofilled_remote_name(
    current_value: &str,
    previous_autofill: Option<&str>,
    suggestion: Option<String>,
) -> Option<String> {
    let current_value = current_value.trim();
    if !(current_value.is_empty() || previous_autofill == Some(current_value)) {
        return None;
    }

    Some(suggestion.unwrap_or_default())
}

pub struct RemoteDialogRequest<'a> {
    pub window: &'a ApplicationWindow,
    pub store: &'a str,
    pub title: &'a str,
    pub initial_name: &'a str,
    pub initial_url: &'a str,
    pub existing_names: Vec<String>,
    pub existing_urls: Vec<String>,
}

pub fn present_remote_dialog(
    request: RemoteDialogRequest<'_>,
    on_submit: impl Fn(String, String) -> Result<(), String> + 'static,
) {
    let existing_names = Rc::new(request.existing_names);
    let existing_urls = Rc::new(request.existing_urls);
    let name_row = EntryRow::new();
    name_row.set_title(&gettext("Remote name"));
    name_row.set_text(request.initial_name);
    let url_row = EntryRow::new();
    url_row.set_title(&gettext("Remote URL"));
    url_row.set_text(request.initial_url);
    url_row.set_show_apply_button(true);

    let syncing = Rc::new(Cell::new(false));
    let last_autofilled_name = Rc::new(RefCell::new(None::<String>));
    {
        let name_row = name_row.clone();
        let syncing = syncing.clone();
        let last_autofilled_name = last_autofilled_name.clone();
        let existing_names = existing_names.clone();
        url_row.connect_changed(move |row| {
            if syncing.get() {
                return;
            }

            let next_name = next_autofilled_remote_name(
                &name_row.text(),
                last_autofilled_name.borrow().as_deref(),
                suggested_remote_name_from_url(&row.text(), existing_names.as_slice()),
            );
            let Some(name) = next_name else {
                last_autofilled_name.borrow_mut().take();
                return;
            };

            let tracked_name = (!name.is_empty()).then_some(name.clone());
            syncing.set(true);
            name_row.set_text(&name);
            syncing.set(false);
            last_autofilled_name.replace(tracked_name);
        });
    }
    sync_remote_dialog_apply_button(&name_row, &url_row);
    {
        let name_row_for_signal = name_row.clone();
        let name_row_for_sync = name_row.clone();
        let url_row_for_sync = url_row.clone();
        name_row_for_signal.connect_changed(move |_| {
            sync_remote_dialog_apply_button(&name_row_for_sync, &url_row_for_sync);
        });
    }
    {
        let url_row_for_signal = url_row.clone();
        let name_row_for_sync = name_row.clone();
        let url_row_for_sync = url_row.clone();
        url_row_for_signal.connect_changed(move |_| {
            sync_remote_dialog_apply_button(&name_row_for_sync, &url_row_for_sync);
        });
    }

    let group = PreferencesGroup::builder().build();
    group.add(&name_row);
    group.add(&url_row);
    let page = PreferencesPage::new();
    page.add(&group);

    let error_label = Label::new(None);
    error_label.set_halign(Align::Start);
    error_label.set_wrap(true);
    error_label.add_css_class("error");
    error_label.add_css_class("caption");
    error_label.set_margin_top(6);
    error_label.set_margin_start(18);
    error_label.set_margin_end(18);
    error_label.set_margin_bottom(18);
    error_label.set_visible(false);

    let content = GtkBox::new(Orientation::Vertical, 0);
    content.append(&page);
    content.append(&error_label);

    let dialog = Dialog::builder()
        .title(gettext(request.title))
        .content_height(280)
        .content_width(800)
        .follows_content_size(true)
        .child(&dialog_content_shell(
            request.title,
            Some(request.store),
            &content,
        ))
        .build();

    let dialog_for_submit = dialog.clone();
    let name_row_for_submit = name_row.clone();
    let existing_names_for_submit = existing_names.clone();
    let existing_urls_for_submit = existing_urls.clone();
    let error_label_for_submit = error_label.clone();
    url_row.connect_apply(move |row| {
        let name = name_row_for_submit.text().trim().to_string();
        let url = row.text().trim().to_string();
        if let Some(message) = remote_dialog_error_message(
            &name,
            &url,
            existing_names_for_submit.as_slice(),
            existing_urls_for_submit.as_slice(),
        ) {
            error_label_for_submit.set_label(&gettext(message));
            error_label_for_submit.set_visible(true);
            return;
        }
        error_label_for_submit.set_visible(false);

        match on_submit(name, url) {
            Ok(()) => {
                dialog_for_submit.close();
            }
            Err(err) => {
                log_error(format!("Git remote dialog failed: {err}"));
                error_label_for_submit.set_label(&gettext("Couldn't save that remote."));
                error_label_for_submit.set_visible(true);
            }
        }
    });

    {
        let error_label = error_label.clone();
        name_row.connect_changed(move |_| error_label.set_visible(false));
    }
    {
        let error_label = error_label.clone();
        url_row.connect_changed(move |_| error_label.set_visible(false));
    }

    dialog.present(Some(request.window));
}

#[doc(hidden)]
pub fn remote_dialog_apply_enabled(name: &str, url: &str) -> bool {
    !name.trim().is_empty() && !url.trim().is_empty()
}

fn sync_remote_dialog_apply_button(name_row: &EntryRow, url_row: &EntryRow) {
    url_row.set_show_apply_button(remote_dialog_apply_enabled(
        &name_row.text(),
        &url_row.text(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_name_autofill_suggests_origin_for_non_empty_urls() {
        assert_eq!(
            suggested_remote_name_from_url("ssh://git@example.test/repo.git", &[]),
            Some("origin".to_string())
        );
        assert_eq!(suggested_remote_name_from_url("", &[]), None);
    }

    #[test]
    fn remote_name_autofill_only_updates_empty_or_last_autofilled_values() {
        assert_eq!(
            next_autofilled_remote_name(
                "",
                None,
                suggested_remote_name_from_url("ssh://git@example.test/repo.git", &[]),
            ),
            Some("origin".to_string())
        );
        assert_eq!(
            next_autofilled_remote_name(
                "origin",
                Some("origin"),
                suggested_remote_name_from_url("ssh://git@example.test/other.git", &[]),
            ),
            Some("origin".to_string())
        );
        assert_eq!(
            next_autofilled_remote_name(
                "custom",
                Some("origin"),
                suggested_remote_name_from_url("ssh://git@example.test/repo.git", &[]),
            ),
            None
        );
    }

    #[test]
    fn duplicate_remote_names_receive_a_stable_suffix() {
        let existing = vec!["origin".to_string(), "origin-2".to_string()];
        assert_eq!(next_available_remote_name("origin", &existing), "origin-3");
        assert_eq!(
            suggested_remote_name_from_url("ssh://git@example.test/repo.git", &existing),
            Some("origin-3".to_string())
        );
    }

    #[test]
    fn remote_name_validation_is_case_insensitive() {
        let existing = vec!["origin".to_string(), "upstream".to_string()];
        assert!(remote_name_exists("origin", &existing));
        assert!(remote_name_exists("ORIGIN", &existing));
        assert!(!remote_name_exists("origin-2", &existing));
    }

    #[test]
    fn remote_url_validation_trims_the_candidate() {
        let existing = vec!["ssh://git@example.test/repo.git".to_string()];
        assert!(remote_url_exists(
            " ssh://git@example.test/repo.git ",
            &existing
        ));
        assert!(!remote_url_exists(
            "ssh://git@example.test/other.git",
            &existing
        ));
    }

    #[test]
    fn remote_dialog_validation_reports_the_first_relevant_error() {
        let existing_names = vec!["origin".to_string()];
        let existing_urls = vec!["ssh://git@example.test/repo.git".to_string()];
        assert_eq!(
            remote_dialog_error_message("", "", &existing_names, &existing_urls),
            Some("Enter a remote name.")
        );
        assert_eq!(
            remote_dialog_error_message("origin", "", &existing_names, &existing_urls),
            Some("That remote name already exists.")
        );
        assert_eq!(
            remote_dialog_error_message("upstream", "", &existing_names, &existing_urls),
            Some("Enter a remote URL.")
        );
        assert_eq!(
            remote_dialog_error_message(
                "upstream",
                "ssh://git@example.test/repo.git",
                &existing_names,
                &existing_urls,
            ),
            Some("That remote URL already exists.")
        );
        assert_eq!(
            remote_dialog_error_message(
                "upstream",
                "ssh://git@example.test/other.git",
                &existing_names,
                &existing_urls,
            ),
            None
        );
    }

    #[test]
    fn remote_dialog_apply_requires_name_and_url() {
        assert!(!remote_dialog_apply_enabled("", ""));
        assert!(!remote_dialog_apply_enabled("origin", ""));
        assert!(!remote_dialog_apply_enabled(
            "",
            "ssh://example.test/repo.git"
        ));
        assert!(remote_dialog_apply_enabled(
            "origin",
            "ssh://example.test/repo.git"
        ));
    }
}
