//! GTK dialogs for opened passkey credentials and export requests.

use crate::credential::PasskeyCredential;
use crate::request::{read_opened_passkey_file, OpenedPasskeyFile, PasskeyExportRequestFile};
use adw::gio::prelude::FileExt;
use adw::prelude::*;
use adw::{AlertDialog, ApplicationWindow, ResponseAppearance};
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub enum OpenPasskeyRequest {
    Valid(PasskeyExportRequestFile),
    Import(PasskeyCredential),
    Invalid(String),
}

/// Classifies an application open request, which must contain exactly one local file.
pub fn open_request_from_files(files: &[adw::gio::File]) -> OpenPasskeyRequest {
    let [file] = files else {
        return OpenPasskeyRequest::Invalid(
            "Open exactly one passkey request at a time.".to_string(),
        );
    };
    open_request_file(file)
}

fn open_request_file(file: &adw::gio::File) -> OpenPasskeyRequest {
    let Some(path) = file.path() else {
        return OpenPasskeyRequest::Invalid(
            "Only regular files on the local filesystem are accepted.".to_string(),
        );
    };

    match read_opened_passkey_file(path) {
        Ok(OpenedPasskeyFile::ExportRequest(request)) => OpenPasskeyRequest::Valid(request),
        Ok(OpenedPasskeyFile::Credential(credential)) => OpenPasskeyRequest::Import(credential),
        Err(error) => OpenPasskeyRequest::Invalid(error.to_string()),
    }
}

/// Parses the explicit passkey-open switch or an existing implicit passkey file argument.
pub fn command_line_request(args: &[OsString]) -> Option<OpenPasskeyRequest> {
    let explicit = args
        .get(1)
        .is_some_and(|arg| arg == "--open-passkey-request");
    let argument = if explicit {
        match (args.get(2), args.get(3)) {
            (Some(argument), None) => argument,
            _ => {
                return Some(OpenPasskeyRequest::Invalid(
                    "Use --open-passkey-request with exactly one local file.".to_string(),
                ));
            }
        }
    } else {
        if args.len() != 2 {
            return None;
        }
        args.get(1)?
    };

    let file = argument
        .to_str()
        .filter(|value| value.starts_with("file://"))
        .map_or_else(
            || adw::gio::File::for_path(PathBuf::from(argument)),
            adw::gio::File::for_uri,
        );
    if explicit {
        return Some(open_request_file(&file));
    }

    let path = file.path()?;
    if std::fs::symlink_metadata(&path).is_err() {
        return None;
    }
    match read_opened_passkey_file(path) {
        Ok(OpenedPasskeyFile::ExportRequest(request)) => Some(OpenPasskeyRequest::Valid(request)),
        Ok(OpenedPasskeyFile::Credential(credential)) => {
            Some(OpenPasskeyRequest::Import(credential))
        }
        Err(error) if error.is_not_passkey_request() => None,
        Err(error) => Some(OpenPasskeyRequest::Invalid(error.to_string())),
    }
}

#[derive(Clone)]
pub struct PasskeyDialogCallbacks {
    translate: Rc<dyn Fn(&str) -> String>,
    import: Rc<dyn Fn(PasskeyCredential) -> Result<(), String>>,
}

impl PasskeyDialogCallbacks {
    pub fn new(
        translate: impl Fn(&str) -> String + 'static,
        import: impl Fn(PasskeyCredential) -> Result<(), String> + 'static,
    ) -> Self {
        Self {
            translate: Rc::new(translate),
            import: Rc::new(import),
        }
    }

    fn text(&self, message: &str) -> String {
        (self.translate)(message)
    }
}

pub fn present_open_passkey_request(
    window: &ApplicationWindow,
    opened: OpenPasskeyRequest,
    callbacks: PasskeyDialogCallbacks,
) {
    if let OpenPasskeyRequest::Import(credential) = opened {
        present_passkey_import(window, credential, callbacks);
        return;
    }

    let (heading, body) = match opened {
        OpenPasskeyRequest::Valid(opened) => {
            let heading = callbacks.text("Passkey export request opened");
            let body = callbacks.text(
                "Keycord opened a credential export request that names {importer} as the importer and checked its structure. The local file does not authenticate that identity. Keycord does not yet generate CXP response archives, so no passkey data was released.",
            )
            .replace("{importer}", &opened.request.importer);
            (heading, body)
        }
        OpenPasskeyRequest::Invalid(error) => {
            let heading = callbacks.text("Couldn't open passkey request");
            let body = callbacks
                .text("The selected file is not a supported local CXP passkey request. {error}")
                .replace("{error}", &error);
            (heading, body)
        }
        OpenPasskeyRequest::Import(_) => unreachable!(),
    };

    let dialog = AlertDialog::builder().heading(heading).body(body).build();
    dialog.add_response("close", &callbacks.text("Close"));
    dialog.set_close_response("close");
    dialog.set_default_response(Some("close"));
    dialog.present(Some(window));
}

fn present_passkey_import(
    window: &ApplicationWindow,
    credential: PasskeyCredential,
    callbacks: PasskeyDialogCallbacks,
) {
    let body = callbacks
        .text(
            "Add the passkey for {username} on {rp_id} to your default pass store? You can review it before saving.",
        )
        .replace("{username}", &credential.username)
        .replace("{rp_id}", &credential.rp_id);
    let dialog = AlertDialog::builder()
        .heading(callbacks.text("Import passkey?"))
        .body(body)
        .build();
    dialog.add_responses(&[
        ("cancel", &callbacks.text("Cancel")),
        ("import", &callbacks.text("Import")),
    ]);
    dialog.set_close_response("cancel");
    dialog.set_default_response(Some("cancel"));
    dialog.set_response_appearance("import", ResponseAppearance::Suggested);

    let window_for_import = window.clone();
    let callbacks_for_import = callbacks.clone();
    dialog.connect_response(Some("import"), move |_, _| {
        if let Err(error) = (callbacks_for_import.import)(credential.clone()) {
            present_import_error(&window_for_import, &error, &callbacks_for_import);
        }
    });
    dialog.present(Some(window));
}

fn present_import_error(
    window: &ApplicationWindow,
    error: &str,
    callbacks: &PasskeyDialogCallbacks,
) {
    let body = callbacks
        .text("Keycord couldn't prepare the passkey item. {error}")
        .replace("{error}", error);
    let dialog = AlertDialog::builder()
        .heading(callbacks.text("Couldn't import passkey"))
        .body(body)
        .build();
    dialog.add_response("close", &callbacks.text("Close"));
    dialog.set_close_response("close");
    dialog.present(Some(window));
}

#[cfg(test)]
mod tests {
    use super::{command_line_request, open_request_from_files, OpenPasskeyRequest};
    use std::ffi::OsString;

    #[test]
    fn explicit_open_switch_requires_exactly_one_path() {
        let args = [
            OsString::from("keycord"),
            OsString::from("--open-passkey-request"),
        ];
        assert!(matches!(
            command_line_request(&args),
            Some(OpenPasskeyRequest::Invalid(message))
                if message.contains("exactly one local file")
        ));
    }

    #[test]
    fn missing_implicit_file_is_not_claimed_as_a_passkey_request() {
        let args = [
            OsString::from("keycord"),
            OsString::from("/keycord-test/missing-passkey-file"),
        ];
        assert!(command_line_request(&args).is_none());
    }

    #[test]
    fn application_open_requires_one_file() {
        assert!(matches!(
            open_request_from_files(&[]),
            OpenPasskeyRequest::Invalid(message)
                if message.contains("exactly one passkey request")
        ));
    }
}
