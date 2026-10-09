//! Keep native binary credentials out of the text editor and its actions.
use super::{editor::current_editor_contents, editor::sync_editor_contents, PasswordPageState};
use crate::model::OpenPassFile;
use adw::prelude::*;

pub(super) fn native_passless(state: &PasswordPageState) -> bool {
    #[cfg(feature = "passless")]
    {
        state.native_passless.get()
    }
    #[cfg(not(feature = "passless"))]
    {
        let _ = state;
        false
    }
}
pub(super) fn current_editor_bytes(state: &PasswordPageState) -> Vec<u8> {
    if native_passless(state) {
        state.saved_contents.borrow().to_vec()
    } else {
        current_editor_contents(state).into_bytes()
    }
}
pub(super) fn validate_editor_bytes(contents: &[u8]) -> Result<(), crate::PasswordEntryError> {
    #[cfg(feature = "passless")]
    if keycord_passkey::inspect_passless(contents).is_some() {
        return Ok(());
    }
    std::str::from_utf8(contents)
        .map(|_| ())
        .map_err(|_| crate::PasswordEntryError::other("The entry is not UTF-8 text."))
}
pub(super) fn sync_editor_bytes(
    state: &PasswordPageState,
    contents: &[u8],
    pass_file: Option<&OpenPassFile>,
) {
    #[cfg(feature = "passless")]
    if let Some(credential) = keycord_passkey::inspect_passless(contents) {
        sync_editor_contents(state, "", pass_file);
        state.native_passless.set(true);
        let subtitle = match credential {
            Ok(credential) => {
                let name = credential
                    .user
                    .name
                    .as_deref()
                    .or(credential.user.display_name.as_deref())
                    .unwrap_or_default();
                if name.is_empty() {
                    credential.rp.id.clone()
                } else {
                    format!("{} — {}", name, credential.rp.id)
                }
            }
            Err(_) => keycord_runtime::i18n::gettext("Unsupported or damaged passkey"),
        };
        let row = adw::ActionRow::builder()
            .title("Passless")
            .subtitle(&subtitle)
            .use_markup(false)
            .margin_start(15)
            .margin_end(15)
            .margin_bottom(6)
            .build();
        state.dynamic_box.append(&row);
        state.dynamic_box.set_visible(true);
        sync_native_controls(state);
        return;
    }
    sync_editor_contents(
        state,
        std::str::from_utf8(contents).unwrap_or_default(),
        pass_file,
    );
}

pub(super) fn sync_native_controls(state: &PasswordPageState) {
    state.entry.set_visible(false);
    state.username.set_visible(false);
    state.raw.set_visible(false);
    state.field_add_row.set_visible(false);
    state.clean_button.set_visible(false);
    state.template_button.set_visible(false);
    state.otp_add_button.set_visible(false);
    state.import_private_key_button.set_visible(false);
    state.password_analysis_label.set_visible(false);
    state.generator_settings_button.set_visible(false);
    state
        .editor_save_button
        .set_visible(!state.saved_entry_exists.get());
    state.save.set_visible(!state.saved_entry_exists.get());
}
