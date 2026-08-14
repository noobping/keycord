mod editor;
mod linux;
mod ports;
mod standard;
mod state;

use super::actions::configure_password_save_button;
use super::list::{load_passwords_async, PasswordListActions};
use crate::file::{
    apply_pass_file_template_contents, clean_pass_file_contents,
    new_pass_file_contents_from_template, pass_file_has_missing_template_fields,
    pass_file_has_passkey_storage_field, structured_pass_contents, sync_username_row,
};
use crate::generation::generate_password;
use crate::model::{OpenPassFile, UsernameFallbackError};
use crate::strength::weak_password_reason;
use crate::ui::opened::{
    clear_opened_pass_file, get_opened_pass_file, is_opened_pass_file,
    refresh_opened_pass_file_from_contents, set_opened_pass_file,
};
use crate::ui::undo::push_undo_action;
use crate::undo::restore_saved_entry_action;
use crate::validation::validate_pass_file_email_fields;
use crate::{PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError};
use adw::prelude::*;
use adw::{ApplicationWindow, Dialog, Toast};
use keycord_keys::PrivateKeyError;
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::actions::activate_widget_action;
use keycord_shell::background::{spawn_progress_result_task, spawn_result_task_with_finalizer};
use keycord_shell::clipboard::set_clipboard_text;
use keycord_shell::navigation::{
    HasWindowChrome, NavigationPageId, NavigationPageRoute, PagePresentation, APP_WINDOW_TITLE,
};
use keycord_shell::ui::{
    build_progress_dialog, navigation_stack_is_root, pop_navigation_to_root,
    push_navigation_page_if_needed, visible_navigation_page_is,
};
use keycord_stores::entry_files::normalize_password_entry_label;
use secrecy::SecretString;
use std::rc::Rc;
use std::string::ToString;

use self::editor::{
    add_empty_dynamic_field, add_empty_otp_secret as add_empty_otp_secret_to_editor,
    current_editor_contents, focus_field_add_row, focus_password_row, structured_editor_contents,
    sync_editor_contents,
};
use self::linux as platform;
use self::platform::handle_open_password_entry_error;
pub use self::ports::*;
pub use self::state::PasswordPageState;
use self::state::{
    reset_password_editor, show_password_editor_chrome, show_password_editor_fields,
    show_password_loading_state, show_password_status_message, sync_saved_password_state,
};

fn password_open_failure_message(error: Option<&PasswordEntryError>) -> &'static str {
    error
        .and_then(PasswordEntryError::toast_message)
        .unwrap_or("Can't open this item.")
}

fn password_save_failure_message(error: &PasswordEntryWriteError) -> &'static str {
    error.save_toast_message()
}

pub fn refresh_password_analysis_label(state: &PasswordPageState) {
    if !state.entry.is_visible() {
        state.password_analysis_label.set_visible(false);
        return;
    }

    let Some(description) = weak_password_reason(state.entry.text().as_str()) else {
        state.password_analysis_label.set_visible(false);
        return;
    };

    state.password_analysis_label.set_label(&description);
    state.password_analysis_label.set_visible(true);
}

const fn username_fallback_failure_message(error: UsernameFallbackError) -> &'static str {
    error.toast_message()
}

const OPEN_STATUS_TITLE: &str = "Opening";
const UNLOCK_STATUS_TITLE: &str = "Unlock key";
const WAIT_A_MOMENT: &str = "Wait a moment.";
const ARMORED_PRIVATE_KEY_BEGIN: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----";
const ARMORED_PRIVATE_KEY_END: &str = "-----END PGP PRIVATE KEY BLOCK-----";

fn password_open_progress_description(progress: &PasswordEntryReadProgress) -> String {
    password_entry_progress_description(progress)
}

fn password_entry_progress_description(progress: &PasswordEntryReadProgress) -> String {
    gettext("Step {current}/{total}: touch your key if it blinks.")
        .replace("{current}", &progress.current_step.to_string())
        .replace("{total}", &progress.total_steps.to_string())
}

pub(super) const fn password_open_status_text() -> (&'static str, &'static str) {
    (OPEN_STATUS_TITLE, WAIT_A_MOMENT)
}

pub(super) const fn password_unlock_status_text() -> (&'static str, &'static str) {
    (UNLOCK_STATUS_TITLE, "Unlock your key to continue.")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PasswordPageDisplay {
    Hidden,
    Loading,
    Editor,
}

struct PasswordSaveContext {
    pass_file: OpenPassFile,
    contents: String,
    previous_store: String,
    previous_label: String,
    previous_contents: String,
    previous_entry_exists: bool,
    target_label: Option<String>,
}

fn show_password_open_failure(state: &PasswordPageState, error: Option<&PasswordEntryError>) {
    activate_widget_action(&state.nav, "win.go-home");
    state
        .overlay
        .add_toast(Toast::new(&gettext(password_open_failure_message(error))));
}

const fn should_retry_open_password_entry(
    page_display: PasswordPageDisplay,
    has_opened_pass_file: bool,
) -> bool {
    matches!(page_display, PasswordPageDisplay::Loading) && has_opened_pass_file
}

fn password_page_display(state: &PasswordPageState) -> PasswordPageDisplay {
    if !visible_navigation_page_is(&state.nav, &state.page) {
        return PasswordPageDisplay::Hidden;
    }
    if state.status.is_visible() && !state.entry.is_visible() {
        return PasswordPageDisplay::Loading;
    }

    PasswordPageDisplay::Editor
}

fn validate_password_save_contents(contents: &str) -> Result<(), String> {
    validate_pass_file_email_fields(contents).map_err(ToString::to_string)
}

fn prepared_password_save_contents(
    contents: String,
    clear_empty_fields_before_save: bool,
) -> String {
    if clear_empty_fields_before_save {
        clean_pass_file_contents(&contents)
    } else {
        contents
    }
}

fn prepare_password_save_context(state: &PasswordPageState) -> Result<PasswordSaveContext, String> {
    let pass_file =
        get_opened_pass_file(&state.nav).ok_or_else(|| "Open an item first.".to_string())?;
    let editor_contents = current_editor_contents(state);

    let otp_url = state
        .otp
        .current_url_for_save()
        .map_err(ToString::to_string)?;
    let contents = if visible_navigation_page_is(&state.nav, &state.raw_page) {
        editor_contents
    } else {
        structured_pass_contents(
            &state.entry.text(),
            &state.username.text(),
            otp_url.as_deref(),
            &state.structured_templates.borrow(),
            &state.dynamic_rows.borrow(),
        )
    };
    let contents = prepared_password_save_contents(
        contents,
        (state.ports.preferences.clear_empty_fields_before_save)(),
    );
    let target_label = pass_file
        .updated_label_from_username(&state.username.text())
        .map_err(|err| username_fallback_failure_message(err).to_string())?;
    validate_password_save_contents(&contents)?;

    Ok(PasswordSaveContext {
        previous_store: pass_file.store_path().to_string(),
        previous_label: pass_file.label(),
        previous_contents: state.saved_contents.borrow().clone(),
        previous_entry_exists: state.saved_entry_exists.get(),
        pass_file,
        contents,
        target_label,
    })
}

fn renamed_pass_file_after_save(
    state: &PasswordPageState,
    save_context: &PasswordSaveContext,
    label: &str,
) -> Result<OpenPassFile, PasswordEntryWriteError> {
    let Some(target_label) = save_context
        .target_label
        .as_ref()
        .filter(|target_label| target_label.as_str() != label)
    else {
        return Ok(save_context.pass_file.clone());
    };

    (state.ports.backend.rename_entry)(
        save_context.pass_file.store_path().to_string(),
        label.to_string(),
        target_label.clone(),
    )?;
    let renamed_pass_file = OpenPassFile::from_label_with_mode(
        save_context.pass_file.store_path(),
        target_label,
        save_context.pass_file.username_fallback_mode(),
    );
    set_opened_pass_file(&state.nav, renamed_pass_file.clone());
    Ok(renamed_pass_file)
}

fn finish_password_save(
    state: &PasswordPageState,
    save_context: &PasswordSaveContext,
    active_pass_file: &OpenPassFile,
) {
    let updated_pass_file = refresh_opened_pass_file_from_contents(
        &state.nav,
        active_pass_file,
        &save_context.contents,
    )
    .or_else(|| Some(active_pass_file.clone()));
    show_password_editor_fields(state);
    sync_editor_contents(state, &save_context.contents, updated_pass_file.as_ref());
    sync_saved_password_state(state, &save_context.contents, true);
    let current_label = updated_pass_file
        .as_ref()
        .map_or_else(|| save_context.previous_label.clone(), OpenPassFile::label);
    if !save_context.previous_entry_exists
        || save_context.previous_contents != save_context.contents
        || save_context.previous_label != current_label
    {
        push_undo_action(
            &state.nav,
            restore_saved_entry_action(
                &save_context.previous_store,
                &save_context.previous_label,
                save_context
                    .previous_entry_exists
                    .then_some(save_context.previous_contents.as_str()),
                save_context.pass_file.store_path(),
                &current_label,
            ),
        );
    }
    state.overlay.add_toast(Toast::new(&gettext("Saved.")));
    activate_widget_action(&state.nav, "win.back");
}

fn handle_password_save_result(
    state: &PasswordPageState,
    save_context: &PasswordSaveContext,
    result: Result<(), PasswordEntryWriteError>,
) {
    let label = save_context.pass_file.label();
    match result {
        Ok(()) => match renamed_pass_file_after_save(state, save_context, &label) {
            Ok(active_pass_file) => finish_password_save(state, save_context, &active_pass_file),
            Err(err) => {
                show_password_editor_fields(state);
                refresh_password_analysis_label(state);
                log_error(format!("Failed to move password entry after save: {err}"));
                state
                    .overlay
                    .add_toast(Toast::new(&gettext(err.rename_toast_message())));
            }
        },
        Err(err) => {
            show_password_editor_fields(state);
            refresh_password_analysis_label(state);
            log_error(format!("Failed to save password entry: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext(password_save_failure_message(&err))));
        }
    }
}

pub fn open_password_entry_page(
    state: &PasswordPageState,
    opened_pass_file: OpenPassFile,
    push_page: bool,
) {
    let pass_label = opened_pass_file.label();
    let store_for_thread = opened_pass_file.store_path().to_string();
    set_opened_pass_file(&state.nav, opened_pass_file.clone());

    show_password_loading_state(state, opened_pass_file.title(), &pass_label);
    if push_page {
        push_navigation_page_if_needed(&state.nav, &state.page);
    }

    let label_for_thread = pass_label;
    let state_for_result = state.clone();
    let opened_pass_file_for_result = opened_pass_file.clone();
    let state_for_disconnect = state.clone();
    let opened_pass_file_for_disconnect = opened_pass_file.clone();
    let state_for_progress = state.clone();
    let opened_pass_file_for_progress = opened_pass_file;
    let read_entry_with_progress = state.ports.backend.read_entry_with_progress.clone();
    spawn_progress_result_task(
        move |progress_tx| {
            read_entry_with_progress(store_for_thread, label_for_thread, progress_tx)
        },
        move |progress| {
            if !is_opened_pass_file(&state_for_progress.nav, &opened_pass_file_for_progress) {
                return;
            }
            show_password_status_message(
                &state_for_progress,
                "Opening item",
                &password_open_progress_description(&progress),
            );
        },
        move |result| {
            if !is_opened_pass_file(&state_for_result.nav, &opened_pass_file_for_result) {
                return;
            }

            match result {
                Ok(output) => {
                    let updated_pass_file = refresh_opened_pass_file_from_contents(
                        &state_for_result.nav,
                        &opened_pass_file_for_result,
                        &output,
                    );
                    show_password_editor_fields(&state_for_result);
                    sync_editor_contents(&state_for_result, &output, updated_pass_file.as_ref());
                    sync_saved_password_state(&state_for_result, &output, true);
                    focus_password_row(&state_for_result);
                }
                Err(err) => {
                    log_error(format!("Failed to open password entry: {err}"));
                    if handle_open_password_entry_error(
                        &state_for_result,
                        &opened_pass_file_for_result,
                        &err,
                    ) {
                        return;
                    }

                    show_password_open_failure(&state_for_result, Some(&err));
                }
            }
        },
        move || {
            if !is_opened_pass_file(&state_for_disconnect.nav, &opened_pass_file_for_disconnect) {
                return;
            }
            show_password_open_failure(&state_for_disconnect, None);
        },
    );
}

pub fn begin_new_password_entry(
    state: &PasswordPageState,
    path: &str,
    store_root: Option<String>,
    add_dialog: &Dialog,
) -> Result<(), &'static str> {
    let template_contents =
        new_pass_file_contents_from_template(&(state.ports.preferences.new_pass_file_template)());
    begin_new_password_entry_with_contents(state, path, store_root, &template_contents)?;
    add_dialog.force_close();
    Ok(())
}

pub fn begin_new_password_entry_with_contents(
    state: &PasswordPageState,
    path: &str,
    store_root: Option<String>,
    contents: &str,
) -> Result<(), &'static str> {
    let path = normalize_password_entry_label(path);
    let path = path.as_str();
    if path.is_empty() {
        return Err("Enter a name.");
    }

    let store_root = store_root.unwrap_or_else(|| (state.ports.preferences.default_store)());
    if store_root.trim().is_empty() {
        return Err("Add a store folder first.");
    }
    let opened_pass_file = OpenPassFile::from_label(store_root, path);
    set_opened_pass_file(&state.nav, opened_pass_file.clone());
    let prepared_pass_file =
        refresh_opened_pass_file_from_contents(&state.nav, &opened_pass_file, contents)
            .or_else(|| get_opened_pass_file(&state.nav));

    show_password_editor_chrome(state, "New item", path);
    show_password_editor_fields(state);
    state.otp.clear();
    push_navigation_page_if_needed(&state.nav, &state.page);

    sync_editor_contents(state, contents, prepared_pass_file.as_ref());
    sync_saved_password_state(state, contents, false);
    focus_password_row(state);
    Ok(())
}

pub fn show_raw_pass_file_page(state: &PasswordPageState) {
    let contents = structured_editor_contents(state);
    if pass_file_has_passkey_storage_field(&contents) {
        return;
    }
    state.text.buffer().set_text(&contents);

    let subtitle = get_opened_pass_file(&state.nav).map_or_else(
        || APP_WINDOW_TITLE.to_string(),
        |pass_file| pass_file.label(),
    );
    show_password_editor_chrome(state, "Raw text", &subtitle);

    push_navigation_page_if_needed(&state.nav, &state.raw_page);
}

pub fn add_empty_otp_secret(state: &PasswordPageState) {
    if !state.otp_add_button.is_visible() {
        return;
    }

    add_empty_otp_secret_to_editor(state);
}

pub fn focus_add_pass_field_input(state: &PasswordPageState) {
    if !visible_navigation_page_is(&state.nav, &state.page) || !state.entry.is_visible() {
        return;
    }

    focus_field_add_row(state);
}

pub fn add_pass_field_from_input(state: &PasswordPageState) {
    if !visible_navigation_page_is(&state.nav, &state.page) || !state.entry.is_visible() {
        return;
    }

    match add_empty_dynamic_field(state, &state.field_add_row.text(), None) {
        Ok(()) => state.field_add_row.set_text(""),
        Err(message) => state.overlay.add_toast(Toast::new(&gettext(message))),
    }
}

pub fn refresh_apply_template_button(state: &PasswordPageState) {
    let contents = current_editor_contents(state);
    sync_apply_template_button(state, &contents);
    sync_import_private_key_button(state, &contents);
}

pub fn apply_pass_file_template(state: &PasswordPageState) {
    let editing_structured = visible_navigation_page_is(&state.nav, &state.page);
    let editing_raw = visible_navigation_page_is(&state.nav, &state.raw_page);
    if (!editing_structured || !state.entry.is_visible()) && !editing_raw {
        return;
    }

    let contents = current_editor_contents(state);
    let templated_contents = apply_pass_file_template_contents(
        &contents,
        &(state.ports.preferences.new_pass_file_template)(),
    );
    if templated_contents == contents {
        return;
    }

    let pass_file = get_opened_pass_file(&state.nav);
    let updated_pass_file = pass_file
        .as_ref()
        .and_then(|pass_file| {
            refresh_opened_pass_file_from_contents(&state.nav, pass_file, &templated_contents)
        })
        .or(pass_file);
    sync_editor_contents(state, &templated_contents, updated_pass_file.as_ref());
    state
        .overlay
        .add_toast(Toast::new(&gettext("Added missing template fields.")));
}

fn sync_apply_template_button(state: &PasswordPageState, contents: &str) {
    state
        .template_button
        .set_visible(pass_file_has_missing_template_fields(
            contents,
            &(state.ports.preferences.new_pass_file_template)(),
        ));
}

fn sync_import_private_key_button(state: &PasswordPageState, contents: &str) {
    state
        .import_private_key_button
        .set_visible(armored_private_key_block_from_contents(contents).is_some());
}

fn armored_private_key_block_from_contents(contents: &str) -> Option<&str> {
    let start = contents.find(ARMORED_PRIVATE_KEY_BEGIN)?;
    let remaining = &contents[start..];
    let end = remaining.find(ARMORED_PRIVATE_KEY_END)?;
    let end = start + end + ARMORED_PRIVATE_KEY_END.len();
    contents.get(start..end)
}

fn current_pass_file_private_key_bytes(state: &PasswordPageState) -> Option<Vec<u8>> {
    armored_private_key_block_from_contents(&current_editor_contents(state))
        .map(|block| block.as_bytes().to_vec())
}

fn password_page_window(state: &PasswordPageState) -> Option<ApplicationWindow> {
    state
        .page
        .root()
        .and_then(|root| root.downcast::<ApplicationWindow>().ok())
}

fn handle_private_key_sync_failure(state: &PasswordPageState, err: &str) {
    log_error(format!(
        "Failed to sync private keys with the host after importing from a pass file: {err}"
    ));
    if let Err(save_err) = (state.ports.preferences.disable_private_key_sync)() {
        log_error(format!(
            "Failed to turn off private-key sync after an error: {save_err}"
        ));
    }
    state.overlay.add_toast(Toast::new(&gettext(
        "Couldn't keep private keys synced. Sync was turned off.",
    )));
}

fn sync_imported_private_key_to_host_if_enabled(state: &PasswordPageState) -> bool {
    if !(state.ports.preferences.sync_private_keys_with_host)() {
        return true;
    }

    match (state.ports.keys.sync_to_host)() {
        Ok(()) => true,
        Err(err) => {
            handle_private_key_sync_failure(state, &err);
            false
        }
    }
}

fn finish_private_key_import_from_pass_file(
    state: &PasswordPageState,
    result: Result<(), PrivateKeyError>,
) {
    match result {
        Ok(()) => {
            let _ = sync_imported_private_key_to_host_if_enabled(state);
            activate_widget_action(&state.nav, "win.reload-password-list");
            state
                .overlay
                .add_toast(Toast::new(&gettext("Key imported.")));
        }
        Err(err) => {
            log_error(format!(
                "Failed to import private key from pass file: {err}"
            ));
            state
                .overlay
                .add_toast(Toast::new(&gettext(err.import_message())));
        }
    }
}

fn start_private_key_import_from_pass_file(
    state: &PasswordPageState,
    bytes: Vec<u8>,
    passphrase: Option<SecretString>,
) {
    let Some(window) = password_page_window(state) else {
        log_error("Private key import was requested without a window root.".to_string());
        state
            .overlay
            .add_toast(Toast::new(&gettext("Couldn't import the key.")));
        return;
    };

    let state = state.clone();
    let progress_dialog = build_progress_dialog(&window, "Importing key", None, WAIT_A_MOMENT);
    let state_for_disconnect = state.clone();
    let import_private_key = state.ports.keys.import_private_key.clone();
    spawn_result_task_with_finalizer(
        move || import_private_key(bytes, passphrase),
        move || progress_dialog.force_close(),
        move |result| {
            finish_private_key_import_from_pass_file(&state, result);
        },
        move || {
            log_error("Pass-file private key import worker disconnected unexpectedly.".to_string());
            state_for_disconnect
                .overlay
                .add_toast(Toast::new(&gettext("Couldn't import the key.")));
        },
    );
}

fn prompt_private_key_passphrase_from_pass_file(state: &PasswordPageState, bytes: Vec<u8>) {
    let Some(window) = password_page_window(state) else {
        log_error("Private key passphrase dialog was requested without a window root.".to_string());
        state
            .overlay
            .add_toast(Toast::new(&gettext("Couldn't import the key.")));
        return;
    };

    let bytes = Rc::new(bytes);
    let overlay = state.overlay.clone();
    let prompt_passphrase = state.ports.keys.prompt_passphrase.clone();
    let state_for_submit = state.clone();
    let on_submit: Rc<dyn Fn(SecretString)> = Rc::new(move |passphrase| {
        start_private_key_import_from_pass_file(
            &state_for_submit,
            bytes.as_slice().to_vec(),
            Some(passphrase),
        );
    });
    prompt_passphrase(&window, &overlay, on_submit);
}

pub fn import_private_key_from_current_pass_file(state: &PasswordPageState) {
    let editing_structured = visible_navigation_page_is(&state.nav, &state.page);
    let editing_raw = visible_navigation_page_is(&state.nav, &state.raw_page);
    if (!editing_structured || !state.entry.is_visible()) && !editing_raw {
        return;
    }

    let Some(bytes) = current_pass_file_private_key_bytes(state) else {
        state.overlay.add_toast(Toast::new(&gettext(
            "This item does not contain an armored private key.",
        )));
        return;
    };

    match (state.ports.keys.requires_passphrase)(bytes.clone()) {
        Ok(true) => prompt_private_key_passphrase_from_pass_file(state, bytes),
        Ok(false) => start_private_key_import_from_pass_file(state, bytes, None),
        Err(err) => {
            log_error(format!("Failed to inspect private key in pass file: {err}"));
            state
                .overlay
                .add_toast(Toast::new(&gettext(err.inspection_message())));
        }
    }
}

pub fn clean_pass_file(state: &PasswordPageState) {
    let editing_structured = visible_navigation_page_is(&state.nav, &state.page);
    let editing_raw = visible_navigation_page_is(&state.nav, &state.raw_page);
    if (!editing_structured || !state.entry.is_visible()) && !editing_raw {
        return;
    }

    let contents = current_editor_contents(state);
    let cleaned_contents = clean_pass_file_contents(&contents);
    if cleaned_contents == contents {
        return;
    }

    let pass_file = get_opened_pass_file(&state.nav);
    let updated_pass_file = pass_file
        .as_ref()
        .and_then(|pass_file| {
            refresh_opened_pass_file_from_contents(&state.nav, pass_file, &cleaned_contents)
        })
        .or(pass_file);
    sync_editor_contents(state, &cleaned_contents, updated_pass_file.as_ref());
    state
        .overlay
        .add_toast(Toast::new(&gettext("Removed empty fields.")));
}

pub fn password_page_has_unsaved_changes(state: &PasswordPageState) -> bool {
    current_editor_contents(state) != *state.saved_contents.borrow()
}

#[cfg(feature = "passkey")]
pub fn password_page_would_discard_work(state: &PasswordPageState) -> bool {
    password_work_would_be_discarded(
        password_page_has_unsaved_changes(state),
        state.saved_entry_exists.get(),
        get_opened_pass_file(&state.nav).is_some(),
    )
}

#[cfg(feature = "passkey")]
const fn password_work_would_be_discarded(
    contents_changed: bool,
    saved_entry_exists: bool,
    has_opened_entry: bool,
) -> bool {
    contents_changed || (has_opened_entry && !saved_entry_exists)
}

pub fn revert_unsaved_password_changes(state: &PasswordPageState) -> bool {
    if !password_page_has_unsaved_changes(state) {
        return false;
    }

    let saved_contents = state.saved_contents.borrow().clone();
    let pass_file = get_opened_pass_file(&state.nav);
    sync_editor_contents(state, &saved_contents, pass_file.as_ref());
    state.overlay.add_toast(Toast::new(&gettext("Reverted.")));
    true
}

#[cfg(all(test, feature = "passkey"))]
mod replacement_tests {
    use super::password_work_would_be_discarded;

    #[test]
    fn second_import_is_blocked_when_the_first_new_entry_is_untouched() {
        assert!(password_work_would_be_discarded(false, false, true));
        assert!(password_work_would_be_discarded(true, true, true));
        assert!(!password_work_would_be_discarded(false, true, true));
        assert!(!password_work_would_be_discarded(false, false, false));
    }
}

pub fn generate_password_entry(state: &PasswordPageState) {
    if !state.entry.is_visible() {
        return;
    }

    let password = generate_password(&state.generator_controls.settings());
    state.entry.set_text(&password);
    refresh_password_analysis_label(state);
    if !visible_navigation_page_is(&state.nav, &state.raw_page) {
        state
            .text
            .buffer()
            .set_text(&structured_editor_contents(state));
    }
}

pub fn toggle_password_options(state: &PasswordPageState) {
    if !visible_navigation_page_is(&state.nav, &state.page) || !state.entry.is_visible() {
        return;
    }

    state
        .generator_settings_button
        .set_active(!state.generator_settings_button.is_active());
}

pub fn copy_current_password(state: &PasswordPageState) {
    let editing_structured = visible_navigation_page_is(&state.nav, &state.page);
    let editing_raw = visible_navigation_page_is(&state.nav, &state.raw_page);
    if (!editing_structured || !state.entry.is_visible()) && !editing_raw {
        return;
    }

    let password = current_editor_contents(state)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string();
    if set_clipboard_text(&password, &state.overlay, None) {
        state.overlay.add_toast(Toast::new(&gettext("Copied.")));
    }
}

pub fn copy_current_username(state: &PasswordPageState) {
    if !visible_navigation_page_is(&state.nav, &state.page) || !state.username.is_visible() {
        return;
    }

    if set_clipboard_text(state.username.text().as_str(), &state.overlay, None) {
        state.overlay.add_toast(Toast::new(&gettext("Copied.")));
    }
}

pub fn copy_current_otp(state: &PasswordPageState) {
    if !visible_navigation_page_is(&state.nav, &state.page) || !state.otp.row.is_visible() {
        return;
    }

    if set_clipboard_text(state.otp.row.text().as_str(), &state.overlay, None) {
        state.overlay.add_toast(Toast::new(&gettext("Copied.")));
    }
}

fn save_current_password_entry_impl(state: &PasswordPageState, allow_git_unlock_prompt: bool) {
    let save_context = match prepare_password_save_context(state) {
        Ok(save_context) => save_context,
        Err(message) => {
            state.overlay.add_toast(Toast::new(&gettext(&message)));
            return;
        }
    };

    if allow_git_unlock_prompt
        && platform::prompt_unlock_for_git_commit_if_needed(state, &save_context.pass_file)
    {
        return;
    }
    let result = (state.ports.backend.save_entry)(
        save_context.pass_file.store_path().to_string(),
        save_context.pass_file.label(),
        save_context.contents.clone(),
        true,
    );
    handle_password_save_result(state, &save_context, result);
}

pub fn save_current_password_entry(state: &PasswordPageState) {
    save_current_password_entry_impl(state, true);
}

pub(super) fn save_current_password_entry_without_git_unlock_prompt(state: &PasswordPageState) {
    save_current_password_entry_impl(state, false);
}

pub const PASSWORD_PAGE_ID: NavigationPageId = NavigationPageId::new("password");
pub const RAW_TEXT_PAGE_ID: NavigationPageId = NavigationPageId::new("raw-text");
pub const PASSWORD_LIST_SUBTITLE: &str = "Browse and edit password stores";

pub fn entry_page_navigation_routes(state: &PasswordPageState) -> [NavigationPageRoute; 2] {
    let opened_pass_file = get_opened_pass_file(&state.nav);
    let password_presentation = opened_pass_file.as_ref().map_or_else(
        || PagePresentation::secondary(APP_WINDOW_TITLE, PASSWORD_LIST_SUBTITLE, true),
        |pass_file| {
            PagePresentation::secondary(pass_file.title(), pass_file.label(), true)
                .with_raw_visible(true)
        },
    );
    let password_state = state.clone();
    let pass_file_for_username = opened_pass_file.clone();
    let raw_save = state.save.clone();

    [
        NavigationPageRoute::secondary(PASSWORD_PAGE_ID, &state.page, password_presentation)
            .with_after_restore(move || {
                configure_password_save_button(&password_state.save);
                sync_username_row(&password_state.username, pass_file_for_username.as_ref());
            }),
        NavigationPageRoute::secondary(
            RAW_TEXT_PAGE_ID,
            &state.raw_page,
            PagePresentation::secondary(
                "Raw text",
                opened_pass_file.as_ref().map_or_else(
                    || APP_WINDOW_TITLE.to_string(),
                    |pass_file| pass_file.label(),
                ),
                true,
            ),
        )
        .with_after_restore(move || configure_password_save_button(&raw_save)),
    ]
}

pub fn show_password_list_page(
    state: &PasswordPageState,
    show_hidden: bool,
    show_duplicates: bool,
) {
    pop_navigation_to_root(&state.nav);

    clear_opened_pass_file(&state.nav);
    let chrome = state.window_chrome();
    (state.ports.show_root_page)(&chrome);

    reset_password_editor(state);

    let list_actions = PasswordListActions::new(
        &state.add,
        &state.primary_action,
        &state.secondary_action,
        &state.find,
        &state.save,
    );
    load_passwords_async(
        &state.list,
        &list_actions,
        &state.overlay,
        Rc::new({
            let navigation = state.nav.clone();
            move || navigation_stack_is_root(&navigation)
        }),
        show_hidden,
        show_duplicates,
        &state.ports.list,
    );
    if let Some(root) = state.list.root() {
        if let Ok(window) = root.downcast::<adw::ApplicationWindow>() {
            (state.ports.sync_tools_action_availability)(&window);
        }
    }
}

pub fn retry_open_password_entry_if_needed(state: &PasswordPageState) -> bool {
    let pass_file = get_opened_pass_file(&state.nav);
    if !should_retry_open_password_entry(password_page_display(state), pass_file.is_some()) {
        return false;
    }

    let Some(pass_file) = pass_file else {
        log_error("Retry-open was requested without an opened pass file.");
        return false;
    };
    open_password_entry_page(state, pass_file, false);
    true
}

#[cfg(test)]
mod tests {
    use super::{
        armored_private_key_block_from_contents, password_open_failure_message,
        password_open_progress_description, password_open_status_text,
        password_save_failure_message, password_unlock_status_text,
        prepared_password_save_contents, should_retry_open_password_entry,
        validate_password_save_contents, PasswordPageDisplay, OPEN_STATUS_TITLE,
        UNLOCK_STATUS_TITLE, WAIT_A_MOMENT,
    };
    use crate::model::{OpenPassFile, UsernameFallbackError};
    use crate::{PasswordEntryError, PasswordEntryReadProgress, PasswordEntryWriteError};
    use keycord_preferences::UsernameFallbackMode;

    fn expected_missing_private_key_open_failure_message() -> &'static str {
        "Add a private key in Preferences."
    }

    #[test]
    fn retry_open_requires_a_hidden_editor_on_the_password_page_with_an_open_item() {
        assert!(should_retry_open_password_entry(
            PasswordPageDisplay::Loading,
            true,
        ));
        assert!(!should_retry_open_password_entry(
            PasswordPageDisplay::Hidden,
            true,
        ));
        assert!(!should_retry_open_password_entry(
            PasswordPageDisplay::Editor,
            true,
        ));
        assert!(!should_retry_open_password_entry(
            PasswordPageDisplay::Loading,
            false,
        ));
    }

    #[test]
    fn password_open_failure_message_falls_back_without_a_specific_error() {
        assert_eq!(password_open_failure_message(None), "Can't open this item.");
        assert_eq!(
            password_open_failure_message(Some(&PasswordEntryError::other("boom"))),
            "Can't open this item."
        );
    }

    #[test]
    fn password_open_failure_message_uses_specific_private_key_toasts_when_available() {
        assert_eq!(
            password_open_failure_message(Some(&PasswordEntryError::missing_private_key(
                "missing"
            ))),
            expected_missing_private_key_open_failure_message()
        );
        assert_eq!(
            password_open_failure_message(Some(&PasswordEntryError::incompatible_private_key(
                "incompatible"
            ))),
            "This key can't open your items."
        );
    }

    #[test]
    fn password_save_failure_message_uses_typed_write_error_mapping() {
        assert_eq!(
            password_save_failure_message(&PasswordEntryWriteError::already_exists("duplicate")),
            "An item with that name already exists."
        );
        assert_eq!(
            password_save_failure_message(&PasswordEntryWriteError::LockedPrivateKey(
                "locked".to_string(),
            )),
            "Unlock the key in Preferences."
        );
    }

    #[test]
    fn password_open_progress_description_shows_step_counts() {
        assert_eq!(
            password_open_progress_description(&PasswordEntryReadProgress {
                current_step: 1,
                total_steps: 2,
            }),
            "Step 1/2: touch your key if it blinks."
        );
    }

    #[test]
    fn password_open_status_text_uses_wait_copy() {
        assert_eq!(
            password_open_status_text(),
            (OPEN_STATUS_TITLE, WAIT_A_MOMENT)
        );
    }

    #[test]
    fn password_unlock_status_text_uses_unlock_copy() {
        assert_eq!(
            password_unlock_status_text(),
            (UNLOCK_STATUS_TITLE, "Unlock your key to continue.")
        );
    }

    #[test]
    fn folder_derived_usernames_update_the_pass_file_path_on_save() {
        let pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        assert_eq!(
            pass_file.updated_label_from_username("bob"),
            Ok(Some("work/bob/github".to_string()))
        );
    }

    #[test]
    fn explicit_usernames_do_not_move_the_pass_file_path_on_save() {
        let mut pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Folder,
        );
        pass_file.refresh_from_contents("secret\nusername: bob");
        assert_eq!(pass_file.updated_label_from_username("carol"), Ok(None));
    }

    #[test]
    fn filename_derived_usernames_update_only_the_file_name_on_save() {
        let pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Filename,
        );
        assert_eq!(
            pass_file.updated_label_from_username("gitlab"),
            Ok(Some("work/alice/gitlab".to_string()))
        );
    }

    #[test]
    fn filename_derived_usernames_reject_invalid_names_on_save() {
        let pass_file = OpenPassFile::from_label_with_mode(
            "/tmp/store",
            "work/alice/github",
            UsernameFallbackMode::Filename,
        );
        assert_eq!(
            pass_file.updated_label_from_username(""),
            Err(UsernameFallbackError::EmptyFilename)
        );
    }

    #[test]
    fn pass_file_save_validation_rejects_invalid_email_fields() {
        assert_eq!(
            validate_password_save_contents("secret\nemail: person@example.com"),
            Ok(())
        );
        assert_eq!(
            validate_password_save_contents("secret\nemail: invalid"),
            Err("Email fields must use a valid email address.".to_string())
        );
    }

    #[test]
    fn prepared_password_save_contents_can_auto_clean_empty_fields() {
        assert_eq!(
            prepared_password_save_contents(
                "secret\nusername:\nurl: https://example.com".to_string(),
                true
            ),
            "secret\nurl: https://example.com".to_string()
        );
        assert_eq!(
            prepared_password_save_contents(
                "secret\nusername:\nurl: https://example.com".to_string(),
                false
            ),
            "secret\nusername:\nurl: https://example.com".to_string()
        );
    }

    #[test]
    fn armored_private_key_block_is_extracted_from_surrounding_pass_file_text() {
        let contents = "hunter2\nnotes: keep this\n-----BEGIN PGP PRIVATE KEY BLOCK-----\nabc\n-----END PGP PRIVATE KEY BLOCK-----\nfooter";

        assert_eq!(
            armored_private_key_block_from_contents(contents),
            Some("-----BEGIN PGP PRIVATE KEY BLOCK-----\nabc\n-----END PGP PRIVATE KEY BLOCK-----")
        );
    }

    #[test]
    fn armored_private_key_block_extraction_returns_none_without_a_private_key() {
        assert_eq!(
            armored_private_key_block_from_contents("hunter2\nusername: me"),
            None
        );
    }
}
