//! Shared validation and interaction helpers for Keys-owned generation forms.

use adw::gtk::prelude::*;
use adw::gtk::Editable;
use adw::prelude::*;
use adw::{EntryRow, PasswordEntryRow};
use keycord_runtime::validation::validate_email_address;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub(super) fn validate_name_and_email(
    name: &str,
    email: &str,
) -> Result<(String, String), &'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Enter a name.");
    }

    let email = validate_email_address(email.trim())?;
    Ok((name.to_string(), email))
}

pub(super) fn connect_changed_sync<W>(widget: &W, sync: Rc<dyn Fn()>)
where
    W: IsA<Editable>,
{
    widget.connect_changed(move |_| sync());
}

fn suggested_name_from_email(email: &str) -> Option<String> {
    let suggested = email
        .trim()
        .split_once('@')
        .map(|(local, _)| local.trim())
        .unwrap_or_default();
    (!suggested.is_empty()).then(|| suggested.to_string())
}

fn suggested_email_from_name(name: &str) -> Option<String> {
    let suggested = name.trim();
    (!suggested.is_empty()).then(|| format!("{suggested}@pass.store"))
}

fn next_autofilled_value(
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

pub(super) fn connect_generation_autofill_rows(name_row: &EntryRow, email_row: &EntryRow) {
    let name_row = name_row.clone();
    let email_row = email_row.clone();
    let syncing = Rc::new(Cell::new(false));
    let last_autofilled_name = Rc::new(RefCell::new(None::<String>));
    let last_autofilled_email = Rc::new(RefCell::new(None::<String>));

    {
        let name_row = name_row.clone();
        let syncing = syncing.clone();
        let last_autofilled_name = last_autofilled_name.clone();
        email_row.connect_changed(move |row| {
            if syncing.get() {
                return;
            }

            let next_name = next_autofilled_value(
                &name_row.text(),
                last_autofilled_name.borrow().as_deref(),
                suggested_name_from_email(&row.text()),
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

    {
        let email_row = email_row.clone();
        let last_autofilled_email = last_autofilled_email.clone();
        name_row.connect_changed(move |row| {
            if syncing.get() {
                return;
            }

            let next_email = next_autofilled_value(
                &email_row.text(),
                last_autofilled_email.borrow().as_deref(),
                suggested_email_from_name(&row.text()),
            );
            let Some(email) = next_email else {
                last_autofilled_email.borrow_mut().take();
                return;
            };

            let tracked_email = (!email.is_empty()).then_some(email.clone());
            syncing.set(true);
            email_row.set_text(&email);
            syncing.set(false);
            last_autofilled_email.replace(tracked_email);
        });
    }
}

pub(super) fn private_generation_apply_enabled(
    name: &str,
    email: &str,
    passphrase: &str,
    confirmation: &str,
) -> bool {
    all_nonempty([name, email, passphrase, confirmation])
}

pub(super) fn hardware_generation_apply_enabled(
    name: &str,
    email: &str,
    admin_pin: &str,
    user_pin: &str,
) -> bool {
    all_nonempty([name, email, admin_pin, user_pin])
}

fn all_nonempty<const N: usize>(values: [&str; N]) -> bool {
    values.iter().all(|value| !value.trim().is_empty())
}

pub(super) fn connect_private_apply_visibility(
    name_row: &EntryRow,
    email_row: &EntryRow,
    password_row: &PasswordEntryRow,
    confirm_row: &PasswordEntryRow,
) {
    let name = name_row.clone();
    let email = email_row.clone();
    let password = password_row.clone();
    let confirmation = confirm_row.clone();
    let sync: Rc<dyn Fn()> = Rc::new(move || {
        confirmation.set_show_apply_button(private_generation_apply_enabled(
            &name.text(),
            &email.text(),
            &password.text(),
            &confirmation.text(),
        ));
    });

    sync();
    connect_changed_sync(name_row, sync.clone());
    connect_changed_sync(email_row, sync.clone());
    connect_changed_sync(password_row, sync.clone());
    connect_changed_sync(confirm_row, sync);
}

pub(super) fn connect_hardware_apply_visibility(
    name_row: &EntryRow,
    email_row: &EntryRow,
    admin_pin_row: &PasswordEntryRow,
    user_pin_row: &PasswordEntryRow,
) {
    let name = name_row.clone();
    let email = email_row.clone();
    let admin_pin = admin_pin_row.clone();
    let user_pin = user_pin_row.clone();
    let sync: Rc<dyn Fn()> = Rc::new(move || {
        user_pin.set_show_apply_button(hardware_generation_apply_enabled(
            &name.text(),
            &email.text(),
            &admin_pin.text(),
            &user_pin.text(),
        ));
    });

    sync();
    connect_changed_sync(name_row, sync.clone());
    connect_changed_sync(email_row, sync.clone());
    connect_changed_sync(admin_pin_row, sync.clone());
    connect_changed_sync(user_pin_row, sync);
}

#[cfg(test)]
mod tests {
    use super::{
        hardware_generation_apply_enabled, next_autofilled_value, private_generation_apply_enabled,
        suggested_email_from_name, suggested_name_from_email,
    };

    #[test]
    fn autofill_helpers_only_replace_empty_or_previous_suggestions() {
        assert_eq!(
            next_autofilled_value("", None, Some("Alice".to_string())),
            Some("Alice".to_string())
        );
        assert_eq!(
            next_autofilled_value("custom", None, Some("Alice".to_string())),
            None
        );
        assert_eq!(
            next_autofilled_value("Alice", Some("Alice"), Some("Bob".to_string())),
            Some("Bob".to_string())
        );
    }

    #[test]
    fn autofill_suggestions_match_expected_patterns() {
        assert_eq!(
            suggested_name_from_email("alice@example.com").as_deref(),
            Some("alice")
        );
        assert_eq!(
            suggested_email_from_name("Alice Example").as_deref(),
            Some("Alice Example@pass.store")
        );
    }

    #[test]
    fn generation_apply_buttons_require_every_field() {
        assert!(!private_generation_apply_enabled(
            "",
            "user@example.com",
            "hunter2",
            "hunter2"
        ));
        assert!(private_generation_apply_enabled(
            "User",
            "user@example.com",
            "hunter2",
            "hunter2"
        ));
        assert!(!hardware_generation_apply_enabled(
            "User",
            "user@example.com",
            "12345678",
            ""
        ));
        assert!(hardware_generation_apply_enabled(
            "User",
            "user@example.com",
            "12345678",
            "123456"
        ));
    }
}
