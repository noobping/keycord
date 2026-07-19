use crate::i18n::gettext;
use crate::password::passkey::PasskeyCredential;
use crate::password::passkey_request::PasskeyExportRequestFile;
use adw::prelude::*;
use adw::{AlertDialog, ApplicationWindow, ResponseAppearance};

#[derive(Clone, Debug)]
pub enum OpenPasskeyRequest {
    Valid(PasskeyExportRequestFile),
    Import(PasskeyCredential),
    Invalid(String),
}

pub fn present_open_passkey_request(window: &ApplicationWindow, opened: OpenPasskeyRequest) {
    if let OpenPasskeyRequest::Import(credential) = opened {
        present_passkey_import(window, credential);
        return;
    }

    let (heading, body) = match opened {
        OpenPasskeyRequest::Valid(opened) => {
            let heading = gettext("Passkey export request opened");
            let body = gettext(
                "Keycord opened a credential export request that names {importer} as the importer and checked its structure. The local file does not authenticate that identity. Keycord does not yet generate CXP response archives, so no passkey data was released.",
            )
            .replace("{importer}", &opened.request.importer);
            (heading, body)
        }
        OpenPasskeyRequest::Invalid(error) => {
            let heading = gettext("Couldn't open passkey request");
            let body =
                gettext("The selected file is not a supported local CXP passkey request. {error}")
                    .replace("{error}", &error);
            (heading, body)
        }
        OpenPasskeyRequest::Import(_) => unreachable!(),
    };

    let dialog = AlertDialog::builder().heading(heading).body(body).build();
    dialog.add_response("close", &gettext("Close"));
    dialog.set_close_response("close");
    dialog.set_default_response(Some("close"));
    dialog.present(Some(window));
}

fn present_passkey_import(window: &ApplicationWindow, credential: PasskeyCredential) {
    let body = gettext(
        "Add the passkey for {username} on {rp_id} to your default pass store? You can review it before saving.",
    )
    .replace("{username}", &credential.username)
    .replace("{rp_id}", &credential.rp_id);
    let dialog = AlertDialog::builder()
        .heading(gettext("Import passkey?"))
        .body(body)
        .build();
    dialog.add_responses(&[
        ("cancel", &gettext("Cancel")),
        ("import", &gettext("Import")),
    ]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("cancel"));
    dialog.set_response_appearance("import", ResponseAppearance::Suggested);

    let window_for_import = window.clone();
    dialog.connect_response(Some("import"), move |_, _| {
        if let Err(error) = crate::window::begin_passkey_import(&window_for_import, &credential) {
            present_import_error(&window_for_import, &error);
        }
    });
    dialog.present(Some(window));
}

fn present_import_error(window: &ApplicationWindow, error: &str) {
    let body =
        gettext("Keycord couldn't prepare the passkey item. {error}").replace("{error}", error);
    let dialog = AlertDialog::builder()
        .heading(gettext("Couldn't import passkey"))
        .body(body)
        .build();
    dialog.add_response("close", &gettext("Close"));
    dialog.set_close_response("close");
    dialog.present(Some(window));
}
