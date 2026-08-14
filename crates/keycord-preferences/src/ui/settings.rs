use crate::{
    BackendKind, PasswordGenerationSettings, PasswordListSortMode, Preferences,
    UsernameFallbackMode,
};
use adw::glib;
use adw::gtk::{CheckButton, ListBox, TextView};
use adw::prelude::*;
use adw::{ActionRow, AlertDialog, ComboRow, EntryRow, Toast, ToastOverlay};
use keycord_runtime::capabilities::{has_host_permission, supports_host_command_features};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
use keycord_shell::ui::{
    connect_entry_row_apply_button_to_nonempty_text, focus_first_matching_list_row_in_order,
    list_row_is_keyboard_focusable,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct PreferencesPageControls<C> {
    pub search: super::PreferencesPageSearchState,
    pub template_view: TextView,
    pub clear_empty_fields_before_save_row: ActionRow,
    pub clear_empty_fields_before_save_check: CheckButton,
    pub username_folder_check: CheckButton,
    pub username_filename_check: CheckButton,
    pub password_list_sort_filename_check: CheckButton,
    pub password_list_sort_hybrid_check: CheckButton,
    pub password_list_sort_store_path_check: CheckButton,
    pub generator_controls: C,
    pub overlay: ToastOverlay,
    pub pass_row: EntryRow,
    pub backend_row: ComboRow,
    pub sync_private_keys_row: ActionRow,
    pub sync_private_keys_check: CheckButton,
    pub audit_use_commit_history_recipients_row: ActionRow,
    pub audit_use_commit_history_recipients_check: CheckButton,
}

impl<C: PasswordGenerationControlsPort> PreferencesPageControls<C> {
    pub fn refresh_settings(
        &self,
        preferences: &Preferences,
        private_key_sync_supported: bool,
        audit_supported: bool,
        git_available: bool,
    ) {
        self.pass_row.set_text(&preferences.command_value());
        sync_backend_rows(&self.backend_row, &self.pass_row, preferences);
        sync_private_key_sync_row(
            &self.sync_private_keys_row,
            &self.sync_private_keys_check,
            preferences,
            private_key_sync_supported,
        );
        sync_audit_history_recipient_row(
            &self.audit_use_commit_history_recipients_row,
            &self.audit_use_commit_history_recipients_check,
            preferences,
            audit_supported,
            git_available,
        );
        sync_clear_empty_fields_before_save_check(
            &self.clear_empty_fields_before_save_check,
            preferences.clear_empty_fields_before_save(),
        );
        sync_password_list_sort_checks(
            &self.password_list_sort_filename_check,
            &self.password_list_sort_hybrid_check,
            &self.password_list_sort_store_path_check,
            preferences.password_list_sort_mode(),
        );
        sync_username_fallback_checks(
            &self.username_folder_check,
            &self.username_filename_check,
            preferences.username_fallback_mode(),
        );
        sync_password_generation_controls(
            &self.generator_controls,
            &preferences.password_generation_settings(),
        );
        self.template_view
            .buffer()
            .set_text(&preferences.new_pass_file_template());
    }

    pub fn focus_first_control(&self, leading_lists: &[ListBox]) {
        let leading_lists = leading_lists.to_vec();
        let backend_row = self.backend_row.clone();
        glib::idle_add_local_once(move || {
            if !focus_first_matching_list_row_in_order(
                &leading_lists,
                list_row_is_keyboard_focusable,
            ) && backend_row.is_visible()
            {
                let _ = backend_row.grab_focus();
            }
        });
    }
}

#[cfg(target_os = "linux")]
const AVAILABLE_BACKENDS: &[BackendKind] = &[BackendKind::Integrated, BackendKind::HostCommand];

#[cfg(not(target_os = "linux"))]
const AVAILABLE_BACKENDS: &[BackendKind] = &[BackendKind::Integrated];

const fn available_backend_kinds() -> &'static [BackendKind] {
    AVAILABLE_BACKENDS
}

const fn backend_row_is_visible() -> bool {
    available_backend_kinds().len() > 1
}

fn combo_position_for_backend_kind(backend: BackendKind) -> u32 {
    available_backend_kinds()
        .iter()
        .position(|candidate| *candidate == backend)
        .unwrap_or(0) as u32
}

fn backend_kind_for_combo_position(position: u32) -> BackendKind {
    available_backend_kinds()
        .get(position as usize)
        .copied()
        .unwrap_or(BackendKind::Integrated)
}

fn host_command_preferences_visible(preferences: &Preferences) -> bool {
    supports_host_command_features() && preferences.uses_host_command_backend()
}

fn backend_row_model() -> adw::gtk::StringList {
    let labels = available_backend_kinds()
        .iter()
        .map(|backend| gettext(backend.label()))
        .collect::<Vec<_>>();
    let label_refs = labels.iter().map(String::as_str).collect::<Vec<_>>();
    adw::gtk::StringList::new(&label_refs)
}

pub fn sync_backend_rows(backend_row: &ComboRow, pass_row: &EntryRow, preferences: &Preferences) {
    let backend = preferences.backend_kind();
    let position = combo_position_for_backend_kind(backend);
    let host_backend_controls_available = supports_host_command_features() && has_host_permission();
    if backend_row.selected() != position {
        backend_row.set_selected(position);
    }
    backend_row.set_visible(backend_row_is_visible());
    backend_row.set_sensitive(host_backend_controls_available);
    pass_row.set_visible(host_command_preferences_visible(preferences));
    pass_row.set_sensitive(host_backend_controls_available);
}

pub fn initialize_backend_rows(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    preferences: &Preferences,
) {
    backend_row.set_model(Some(&backend_row_model()));
    sync_backend_rows(backend_row, pass_row, preferences);
}

pub fn connect_pass_command_row(
    pass_row: &EntryRow,
    overlay: &ToastOverlay,
    preferences: &Preferences,
) {
    connect_entry_row_apply_button_to_nonempty_text(pass_row);
    let overlay = overlay.clone();
    let preferences = preferences.clone();
    pass_row.connect_apply(move |row| {
        let text = row.text().to_string();
        let text = text.trim();
        if text.is_empty() {
            overlay.add_toast(Toast::new(&gettext("Enter a command.")));
            return;
        }
        if let Err(err) = preferences.set_command(text) {
            toast_preferences_save_error(&overlay, "host", &err);
        }
    });
}

pub fn connect_backend_row(
    backend_row: &ComboRow,
    pass_row: &EntryRow,
    overlay: &ToastOverlay,
    preferences: &Preferences,
    on_changed: impl Fn() + 'static,
) {
    let overlay = overlay.clone();
    let preferences = preferences.clone();
    let pass_row = pass_row.clone();
    let on_changed = Rc::new(on_changed);
    backend_row.connect_selected_notify(move |row| {
        let selected_backend = backend_kind_for_combo_position(row.selected());
        let current_backend = preferences.backend_kind();
        if selected_backend == current_backend {
            pass_row.set_visible(host_command_preferences_visible(&preferences));
            return;
        }

        if let Err(err) = preferences.set_backend_kind(selected_backend) {
            pass_row.set_visible(host_command_preferences_visible(&preferences));
            row.set_selected(combo_position_for_backend_kind(current_backend));
            toast_preferences_save_error(&overlay, "backend", &err);
            return;
        }

        pass_row.set_visible(host_command_preferences_visible(&preferences));
        on_changed();
    });
}

#[cfg(target_os = "linux")]
const SYNC_PRIVATE_KEYS_AVAILABLE_SUBTITLE: &str =
    "Keep Keycord's private keys and your computer's GPG private keys in step when host access is available.";

#[cfg(target_os = "linux")]
const SYNC_PRIVATE_KEYS_UNAVAILABLE_SUBTITLE: &str =
    "Grant host access first if you want Keycord to keep its private keys in step with your computer's GPG private keys.";

#[cfg(not(target_os = "linux"))]
const SYNC_PRIVATE_KEYS_AVAILABLE_SUBTITLE: &str =
    "Private-key sync with the host is only available on Linux.";

#[cfg(not(target_os = "linux"))]
const SYNC_PRIVATE_KEYS_UNAVAILABLE_SUBTITLE: &str =
    "Private-key sync with the host is only available on Linux.";

fn host_private_key_sync_is_available(supported: bool) -> bool {
    supported && has_host_permission()
}

pub fn sync_private_key_sync_row(
    row: &ActionRow,
    check: &CheckButton,
    preferences: &Preferences,
    supported: bool,
) {
    row.set_visible(supported);
    if !supported {
        return;
    }

    let available = host_private_key_sync_is_available(supported);
    let enabled = preferences.sync_private_keys_with_host();
    row.set_sensitive(available);
    check.set_sensitive(available);
    let subtitle = if available {
        gettext(SYNC_PRIVATE_KEYS_AVAILABLE_SUBTITLE)
    } else {
        gettext(SYNC_PRIVATE_KEYS_UNAVAILABLE_SUBTITLE)
    };
    row.set_subtitle(&subtitle);

    if check.is_active() != enabled {
        check.set_active(enabled);
    }
}

#[derive(Clone)]
pub struct PrivateKeySyncCallbacks {
    sync_host_to_app: Rc<dyn Fn() -> Result<(), String>>,
    on_synced: Rc<dyn Fn()>,
}

impl PrivateKeySyncCallbacks {
    pub fn new(
        sync_host_to_app: impl Fn() -> Result<(), String> + 'static,
        on_synced: impl Fn() + 'static,
    ) -> Self {
        Self {
            sync_host_to_app: Rc::new(sync_host_to_app),
            on_synced: Rc::new(on_synced),
        }
    }
}

fn present_private_key_sync_confirmation(
    window: &adw::ApplicationWindow,
    on_response: impl FnOnce(bool) + 'static,
) {
    let dialog = AlertDialog::builder()
        .heading(gettext("Turn on private-key sync?"))
        .body(gettext("Keycord will first make its private-key list match the GPG private keys on your computer. After that, creating, importing, or deleting a private key in Keycord will update the host too. Keys that only exist in Keycord may be removed during the first sync."))
        .build();
    let cancel = gettext("Cancel");
    let turn_on = gettext("Turn On");
    dialog.add_responses(&[("cancel", cancel.as_str()), ("sync", turn_on.as_str())]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("sync"));
    let on_response = Rc::new(RefCell::new(Some(on_response)));
    dialog.connect_response(None, move |_, response| {
        if let Some(on_response) = on_response.borrow_mut().take() {
            on_response(response == "sync");
        }
    });
    dialog.present(Some(window));
}

pub fn connect_private_key_sync_row(
    row: &ActionRow,
    check: &CheckButton,
    overlay: &ToastOverlay,
    window: &adw::ApplicationWindow,
    supported: bool,
    callbacks: PrivateKeySyncCallbacks,
) {
    let check_for_row = check.clone();
    row.connect_activated(move |_| {
        if check_for_row.is_sensitive() {
            check_for_row.set_active(!check_for_row.is_active());
        }
    });

    let overlay = overlay.clone();
    let window = window.clone();
    let preferences = Preferences::new();
    let syncing = Rc::new(Cell::new(false));
    check.connect_toggled(move |button| {
        if syncing.get() {
            return;
        }

        let desired = button.is_active();
        let stored = preferences.sync_private_keys_with_host();
        if desired == stored {
            return;
        }

        if desired {
            if !host_private_key_sync_is_available(supported) {
                syncing.set(true);
                button.set_active(false);
                syncing.set(false);
                overlay.add_toast(Toast::new(&gettext("Grant host access first.")));
                return;
            }

            let confirm_button = button.clone();
            let confirm_overlay = overlay.clone();
            let confirm_preferences = preferences.clone();
            let confirm_syncing = syncing.clone();
            let callbacks = callbacks.clone();
            present_private_key_sync_confirmation(&window, move |confirmed| {
                if !confirmed {
                    confirm_syncing.set(true);
                    confirm_button.set_active(false);
                    confirm_syncing.set(false);
                    return;
                }

                match (callbacks.sync_host_to_app)() {
                    Ok(()) => {
                        if let Err(err) = confirm_preferences.set_sync_private_keys_with_host(true)
                        {
                            toast_preferences_save_error(
                                &confirm_overlay,
                                "private-key sync",
                                &err,
                            );
                            confirm_syncing.set(true);
                            confirm_button.set_active(false);
                            confirm_syncing.set(false);
                            return;
                        }

                        (callbacks.on_synced)();
                        confirm_overlay.add_toast(Toast::new(&gettext("Private keys synced.")));
                    }
                    Err(err) => {
                        log_error(format!("Failed to enable private-key sync: {err}"));
                        confirm_syncing.set(true);
                        confirm_button.set_active(false);
                        confirm_syncing.set(false);
                        confirm_overlay
                            .add_toast(Toast::new(&gettext("Couldn't sync private keys.")));
                    }
                }
            });
            return;
        }

        if let Err(err) = preferences.set_sync_private_keys_with_host(false) {
            toast_preferences_save_error(&overlay, "private-key sync", &err);
            syncing.set(true);
            button.set_active(true);
            syncing.set(false);
        }
    });
}

pub fn sync_audit_history_recipient_row(
    row: &ActionRow,
    check: &CheckButton,
    preferences: &Preferences,
    supported: bool,
    git_available: bool,
) {
    const AUDIT_HISTORY_RECIPIENTS_SUBTITLE: &str = "Retry authorization with recipient files from the commit history when the current branch recipients cannot authorize the signer.";
    const AUDIT_HISTORY_RECIPIENTS_DISABLED_SUBTITLE: &str =
        "Install Git to use commit-history recipients for audit verification.";

    row.set_visible(supported);
    if !supported {
        return;
    }

    let enabled = preferences.audit_use_commit_history_recipients();
    row.set_sensitive(git_available);
    check.set_sensitive(git_available);
    row.set_subtitle(&gettext(if git_available {
        AUDIT_HISTORY_RECIPIENTS_SUBTITLE
    } else {
        AUDIT_HISTORY_RECIPIENTS_DISABLED_SUBTITLE
    }));
    if check.is_active() != enabled {
        check.set_active(enabled);
    }
}

pub fn connect_audit_history_recipient_row(
    row: &ActionRow,
    check: &CheckButton,
    overlay: &ToastOverlay,
    supported: bool,
    git_available: bool,
) {
    let check_for_row = check.clone();
    row.connect_activated(move |_| {
        if check_for_row.is_sensitive() {
            check_for_row.set_active(!check_for_row.is_active());
        }
    });

    let overlay = overlay.clone();
    let preferences = Preferences::new();
    sync_audit_history_recipient_row(row, check, &preferences, supported, git_available);
    let syncing = Rc::new(Cell::new(false));
    check.connect_toggled(move |button| {
        if syncing.get() {
            return;
        }

        let desired = button.is_active();
        let stored = preferences.audit_use_commit_history_recipients();
        if desired == stored {
            return;
        }

        syncing.set(true);
        if let Err(err) = preferences.set_audit_use_commit_history_recipients(desired) {
            toast_preferences_save_error(&overlay, "History recipients", &err);
            button.set_active(stored);
        }
        syncing.set(false);
    });
}

pub fn toast_preferences_save_error(overlay: &ToastOverlay, context: &str, err: &glib::BoolError) {
    log_error(format!(
        "Failed to save preference ({context}): {}",
        err.message
    ));
    overlay.add_toast(Toast::new(&gettext("Couldn't save that setting.")));
}

pub fn sync_clear_empty_fields_before_save_check(check: &CheckButton, enabled: bool) {
    if check.is_active() != enabled {
        check.set_active(enabled);
    }
}

pub fn connect_clear_empty_fields_before_save_autosave(
    row: &ActionRow,
    check: &CheckButton,
    overlay: &ToastOverlay,
) {
    let check_for_row = check.clone();
    row.connect_activated(move |_| {
        if check_for_row.is_sensitive() {
            check_for_row.set_active(!check_for_row.is_active());
        }
    });

    let overlay = overlay.clone();
    let preferences = Preferences::new();
    sync_clear_empty_fields_before_save_check(check, preferences.clear_empty_fields_before_save());

    let syncing = Rc::new(Cell::new(false));
    let syncing_for_toggle = syncing.clone();
    check.connect_toggled(move |button| {
        if syncing_for_toggle.get() {
            return;
        }

        let desired = button.is_active();
        let stored = preferences.clear_empty_fields_before_save();
        if desired == stored {
            return;
        }

        syncing_for_toggle.set(true);
        if let Err(err) = preferences.set_clear_empty_fields_before_save(desired) {
            toast_preferences_save_error(&overlay, "clear empty fields before save", &err);
            button.set_active(stored);
        }
        syncing_for_toggle.set(false);
    });
}

pub fn connect_new_password_template_autosave(template_view: &TextView, overlay: &ToastOverlay) {
    let overlay = overlay.clone();
    let preferences = Preferences::new();
    template_view.buffer().connect_changed(move |buffer| {
        let (start, end) = buffer.bounds();
        let template = buffer.text(&start, &end, false).to_string();
        if template == preferences.new_pass_file_template() {
            return;
        }
        if let Err(err) = preferences.set_new_pass_file_template(&template) {
            toast_preferences_save_error(&overlay, "new item template", &err);
        }
    });
}

pub fn sync_username_fallback_checks(
    folder_check: &CheckButton,
    filename_check: &CheckButton,
    mode: UsernameFallbackMode,
) {
    let (folder_active, filename_active) = username_fallback_check_state(mode);
    folder_check.set_active(folder_active);
    filename_check.set_active(filename_active);
}

const fn username_fallback_check_state(mode: UsernameFallbackMode) -> (bool, bool) {
    match mode {
        UsernameFallbackMode::Folder => (true, false),
        UsernameFallbackMode::Filename => (false, true),
    }
}

pub fn connect_username_fallback_autosave(
    folder_check: &CheckButton,
    filename_check: &CheckButton,
    overlay: &ToastOverlay,
) {
    let preferences = Preferences::new();
    sync_username_fallback_checks(
        folder_check,
        filename_check,
        preferences.username_fallback_mode(),
    );

    let syncing = Rc::new(Cell::new(false));
    for (button, mode) in [
        (folder_check.clone(), UsernameFallbackMode::Folder),
        (filename_check.clone(), UsernameFallbackMode::Filename),
    ] {
        let folder_check = folder_check.clone();
        let filename_check = filename_check.clone();
        let overlay = overlay.clone();
        let preferences = preferences.clone();
        let syncing = syncing.clone();
        button.connect_toggled(move |button| {
            if syncing.get() || !button.is_active() {
                return;
            }

            let stored = preferences.username_fallback_mode();
            if stored == mode {
                return;
            }

            syncing.set(true);
            if let Err(err) = preferences.set_username_fallback_mode(mode) {
                toast_preferences_save_error(&overlay, "username fallback", &err);
                sync_username_fallback_checks(&folder_check, &filename_check, stored);
            } else {
                sync_username_fallback_checks(&folder_check, &filename_check, mode);
            }
            syncing.set(false);
        });
    }
}

pub fn sync_password_list_sort_checks(
    filename_check: &CheckButton,
    hybrid_check: &CheckButton,
    store_path_check: &CheckButton,
    mode: PasswordListSortMode,
) {
    let (filename_active, hybrid_active, store_path_active) = password_list_sort_check_state(mode);
    filename_check.set_active(filename_active);
    hybrid_check.set_active(hybrid_active);
    store_path_check.set_active(store_path_active);
}

const fn password_list_sort_check_state(mode: PasswordListSortMode) -> (bool, bool, bool) {
    match mode {
        PasswordListSortMode::Filename => (true, false, false),
        PasswordListSortMode::Hybrid => (false, true, false),
        PasswordListSortMode::StorePath => (false, false, true),
    }
}

pub fn connect_password_list_sort_autosave(
    filename_check: &CheckButton,
    hybrid_check: &CheckButton,
    store_path_check: &CheckButton,
    overlay: &ToastOverlay,
    on_changed: impl Fn() + 'static,
) {
    let preferences = Preferences::new();
    sync_password_list_sort_checks(
        filename_check,
        hybrid_check,
        store_path_check,
        preferences.password_list_sort_mode(),
    );

    let syncing = Rc::new(Cell::new(false));
    let on_changed = Rc::new(on_changed);
    for (button, mode) in [
        (filename_check.clone(), PasswordListSortMode::Filename),
        (hybrid_check.clone(), PasswordListSortMode::Hybrid),
        (store_path_check.clone(), PasswordListSortMode::StorePath),
    ] {
        let filename_check = filename_check.clone();
        let hybrid_check = hybrid_check.clone();
        let store_path_check = store_path_check.clone();
        let overlay = overlay.clone();
        let preferences = preferences.clone();
        let syncing = syncing.clone();
        let on_changed = on_changed.clone();
        button.connect_toggled(move |button| {
            if syncing.get() || !button.is_active() {
                return;
            }

            let stored = preferences.password_list_sort_mode();
            if stored == mode {
                return;
            }

            syncing.set(true);
            if let Err(err) = preferences.set_password_list_sort_mode(mode) {
                toast_preferences_save_error(&overlay, "password list sort", &err);
                sync_password_list_sort_checks(
                    &filename_check,
                    &hybrid_check,
                    &store_path_check,
                    stored,
                );
            } else {
                sync_password_list_sort_checks(
                    &filename_check,
                    &hybrid_check,
                    &store_path_check,
                    mode,
                );
                on_changed();
            }
            syncing.set(false);
        });
    }
}

pub trait PasswordGenerationControlsPort: Clone + 'static {
    fn settings(&self) -> PasswordGenerationSettings;
    fn set_settings(&self, settings: &PasswordGenerationSettings);
    fn connect_changed(&self, changed: &Rc<dyn Fn()>);
}

pub fn connect_password_generation_autosave<C: PasswordGenerationControlsPort>(
    controls: &C,
    mirrors: &[C],
    overlay: &ToastOverlay,
) {
    let preferences = Preferences::new();
    let initial_settings = preferences.password_generation_settings();
    sync_password_generation_controls(controls, &initial_settings);
    for mirror in mirrors {
        sync_password_generation_controls(mirror, &initial_settings);
    }

    let controls = controls.clone();
    let changed_controls = controls.clone();
    let mirrors = mirrors.to_vec();
    let overlay = overlay.clone();
    let syncing = Rc::new(Cell::new(false));
    let changed: Rc<dyn Fn()> = Rc::new(move || {
        if syncing.get() {
            return;
        }

        syncing.set(true);
        let stored = preferences.password_generation_settings();
        let updated = changed_controls.settings().normalized();
        match preferences.set_password_generation_settings(&updated) {
            Ok(()) => {
                sync_password_generation_controls(&changed_controls, &updated);
                for mirror in &mirrors {
                    sync_password_generation_controls(mirror, &updated);
                }
            }
            Err(err) => {
                toast_preferences_save_error(&overlay, "password generation", &err);
                sync_password_generation_controls(&changed_controls, &stored);
                for mirror in &mirrors {
                    sync_password_generation_controls(mirror, &stored);
                }
            }
        }
        syncing.set(false);
    });
    controls.connect_changed(&changed);
}

pub fn sync_password_generation_controls<C: PasswordGenerationControlsPort>(
    controls: &C,
    settings: &PasswordGenerationSettings,
) {
    controls.set_settings(settings);
}

#[cfg(test)]
mod tests {
    use super::{
        available_backend_kinds, backend_kind_for_combo_position, combo_position_for_backend_kind,
        password_list_sort_check_state, username_fallback_check_state,
    };
    use crate::{BackendKind, PasswordListSortMode, UsernameFallbackMode};

    #[test]
    fn username_fallback_sync_marks_only_the_selected_mode() {
        assert_eq!(
            username_fallback_check_state(UsernameFallbackMode::Folder),
            (true, false)
        );
        assert_eq!(
            username_fallback_check_state(UsernameFallbackMode::Filename),
            (false, true)
        );
    }

    #[test]
    fn password_list_sort_sync_marks_only_the_selected_mode() {
        assert_eq!(
            password_list_sort_check_state(PasswordListSortMode::Filename),
            (true, false, false)
        );
        assert_eq!(
            password_list_sort_check_state(PasswordListSortMode::Hybrid),
            (false, true, false)
        );
        assert_eq!(
            password_list_sort_check_state(PasswordListSortMode::StorePath),
            (false, false, true)
        );
    }

    #[test]
    fn backend_combo_round_trips_available_backends() {
        for backend in available_backend_kinds() {
            let position = combo_position_for_backend_kind(*backend);
            assert_eq!(backend_kind_for_combo_position(position), *backend);
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_builds_offer_the_host_backend() {
        assert_eq!(
            available_backend_kinds(),
            &[BackendKind::Integrated, BackendKind::HostCommand]
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn non_linux_builds_hide_the_host_backend() {
        assert_eq!(available_backend_kinds(), &[BackendKind::Integrated]);
    }
}
