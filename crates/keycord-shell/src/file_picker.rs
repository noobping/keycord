#[cfg(target_os = "linux")]
use adw::gio;
#[cfg(target_os = "linux")]
use adw::gtk::FileDialog;
#[cfg(target_os = "linux")]
use adw::prelude::*;
use adw::{ApplicationWindow, Toast, ToastOverlay};
use keycord_runtime::i18n::gettext;
use keycord_runtime::log_error;
#[cfg(target_os = "windows")]
use winsafe::{self as w, co, prelude::*};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalPathKind {
    File,
    Folder,
}

impl LocalPathKind {
    const fn chooser_error_message(self) -> &'static str {
        match self {
            Self::File => "Couldn't open the file chooser.",
            Self::Folder => "Couldn't open the folder chooser.",
        }
    }

    #[cfg(any(target_os = "linux", test))]
    const fn local_path_error_message(self) -> &'static str {
        match self {
            Self::File => "Choose a local file.",
            Self::Folder => "Choose a local folder.",
        }
    }
}

#[cfg(target_os = "linux")]
fn selected_local_path(
    file: &gio::File,
    kind: LocalPathKind,
    overlay: &ToastOverlay,
) -> Option<String> {
    let path = file.path().or_else(|| {
        log_error(format!(
            "The selected path is not available locally. {}",
            kind.local_path_error_message()
        ));
        overlay.add_toast(Toast::new(&gettext(kind.local_path_error_message())));
        None
    })?;

    Some(path.to_string_lossy().to_string())
}

#[cfg(target_os = "linux")]
fn choose_local_path_with_dialog(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    kind: LocalPathKind,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    let dialog = FileDialog::builder()
        .title(gettext(title))
        .accept_label(gettext(accept_label))
        .modal(true)
        .build();

    let overlay = overlay.clone();
    let handle_result = move |result: Result<gio::File, adw::glib::Error>| match result {
        Ok(file) => {
            if let Some(path) = selected_local_path(&file, kind, &overlay) {
                on_selected(path);
            }
        }
        Err(err) if err.matches(gio::IOErrorEnum::Cancelled) => {}
        Err(err) => {
            log_error(format!("Failed to open the file chooser: {err}"));
            overlay.add_toast(Toast::new(&gettext(kind.chooser_error_message())));
        }
    };

    match kind {
        LocalPathKind::File => {
            dialog.open(Some(window), None::<&gio::Cancellable>, handle_result);
        }
        LocalPathKind::Folder => {
            dialog.select_folder(Some(window), None::<&gio::Cancellable>, handle_result);
        }
    }
}

#[cfg(target_os = "windows")]
fn choose_windows_path(
    title: &str,
    accept_label: &str,
    kind: LocalPathKind,
    create_folders: bool,
) -> Result<Option<String>, String> {
    let _com = w::CoInitializeEx(co::COINIT::APARTMENTTHREADED)
        .map_err(|err| format!("Failed to initialize COM for the file picker: {err}"))?;
    let dialog = w::CoCreateInstance::<w::IFileOpenDialog>(
        &co::CLSID::FileOpenDialog,
        None::<&w::IUnknown>,
        co::CLSCTX::INPROC_SERVER,
    )
    .map_err(|err| format!("Failed to create the Windows file picker: {err}"))?;

    let mut options = dialog
        .GetOptions()
        .map_err(|err| format!("Failed to read Windows file picker options: {err}"))?
        | co::FOS::FORCEFILESYSTEM;
    match kind {
        LocalPathKind::File => {
            options |= co::FOS::FILEMUSTEXIST;
        }
        LocalPathKind::Folder => {
            options |= co::FOS::PICKFOLDERS;
            if !create_folders {
                options |= co::FOS::PATHMUSTEXIST;
            }
        }
    }

    dialog
        .SetOptions(options)
        .map_err(|err| format!("Failed to configure Windows file picker options: {err}"))?;
    dialog
        .SetTitle(title)
        .map_err(|err| format!("Failed to set the Windows file picker title: {err}"))?;
    dialog
        .SetOkButtonLabel(accept_label)
        .map_err(|err| format!("Failed to set the Windows file picker button label: {err}"))?;

    let owner = w::HWND::GetDesktopWindow();
    let accepted = dialog
        .Show(&owner)
        .map_err(|err| format!("Failed to show the Windows file picker: {err}"))?;
    if !accepted {
        return Ok(None);
    }

    dialog
        .GetResult()
        .and_then(|item| item.GetDisplayName(co::SIGDN::FILESYSPATH))
        .map(Some)
        .map_err(|err| format!("Failed to read the selected Windows path: {err}"))
}

pub fn choose_local_file_path(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    #[cfg(target_os = "linux")]
    choose_local_path_with_dialog(
        window,
        title,
        accept_label,
        LocalPathKind::File,
        overlay,
        on_selected,
    );

    #[cfg(target_os = "windows")]
    {
        let _ = window;
        match choose_windows_path(title, accept_label, LocalPathKind::File, false) {
            Ok(Some(path)) => on_selected(path),
            Ok(None) => {}
            Err(err) => {
                log_error(err);
                overlay.add_toast(Toast::new(&gettext(
                    LocalPathKind::File.chooser_error_message(),
                )));
            }
        }
    }
}

pub fn choose_local_folder_path(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    create_folders: bool,
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    #[cfg(target_os = "linux")]
    // GtkFileDialog delegates folder creation controls to the platform chooser.
    let _ = create_folders;

    #[cfg(target_os = "linux")]
    choose_local_path_with_dialog(
        window,
        title,
        accept_label,
        LocalPathKind::Folder,
        overlay,
        on_selected,
    );

    #[cfg(target_os = "windows")]
    {
        let _ = window;
        match choose_windows_path(title, accept_label, LocalPathKind::Folder, create_folders) {
            Ok(Some(path)) => on_selected(path),
            Ok(None) => {}
            Err(err) => {
                log_error(err);
                overlay.add_toast(Toast::new(&gettext(
                    LocalPathKind::Folder.chooser_error_message(),
                )));
            }
        }
    }
}

pub fn choose_local_save_file_path(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    initial_name: &str,
    file_type: (&str, &str),
    overlay: &ToastOverlay,
    on_selected: impl Fn(String) + 'static,
) {
    let (file_type_label, extension) = file_type;

    #[cfg(target_os = "linux")]
    {
        let _ = (file_type_label, extension);
        let dialog = FileDialog::builder()
            .title(gettext(title))
            .accept_label(gettext(accept_label))
            .initial_name(initial_name)
            .modal(true)
            .build();
        let overlay = overlay.clone();
        dialog.save(
            Some(window),
            None::<&gio::Cancellable>,
            move |result| match result {
                Ok(file) => {
                    if let Some(path) = selected_local_path(&file, LocalPathKind::File, &overlay) {
                        on_selected(path);
                    }
                }
                Err(err) if err.matches(gio::IOErrorEnum::Cancelled) => {}
                Err(err) => {
                    log_error(format!("Failed to open the save-file chooser: {err}"));
                    overlay.add_toast(Toast::new(&gettext("Couldn't open the file chooser.")));
                }
            },
        );
    }

    #[cfg(target_os = "windows")]
    {
        let _ = window;
        match choose_windows_save_path(
            title,
            accept_label,
            initial_name,
            file_type_label,
            extension,
        ) {
            Ok(Some(path)) => on_selected(path),
            Ok(None) => {}
            Err(err) => {
                log_error(err);
                overlay.add_toast(Toast::new(&gettext("Couldn't open the file chooser.")));
            }
        }
    }
}

#[cfg(any(target_os = "windows", test))]
fn save_file_type_spec(file_type_label: &str, extension: &str) -> (String, String) {
    let pattern = format!("*.{extension}");
    (format!("{file_type_label} ({pattern})"), pattern)
}

#[cfg(target_os = "windows")]
fn choose_windows_save_path(
    title: &str,
    accept_label: &str,
    initial_name: &str,
    file_type_label: &str,
    extension: &str,
) -> Result<Option<String>, String> {
    let _com = w::CoInitializeEx(co::COINIT::APARTMENTTHREADED)
        .map_err(|err| format!("Failed to initialize COM for the save-file picker: {err}"))?;
    let dialog = w::CoCreateInstance::<w::IFileSaveDialog>(
        &co::CLSID::FileSaveDialog,
        None::<&w::IUnknown>,
        co::CLSCTX::INPROC_SERVER,
    )
    .map_err(|err| format!("Failed to create the Windows save-file picker: {err}"))?;
    let options = dialog
        .GetOptions()
        .map_err(|err| format!("Failed to read Windows save-file picker options: {err}"))?
        | co::FOS::FORCEFILESYSTEM
        | co::FOS::PATHMUSTEXIST
        | co::FOS::OVERWRITEPROMPT;

    dialog
        .SetOptions(options)
        .map_err(|err| format!("Failed to configure Windows save-file picker options: {err}"))?;
    dialog
        .SetTitle(title)
        .map_err(|err| format!("Failed to set the Windows save-file picker title: {err}"))?;
    dialog
        .SetOkButtonLabel(accept_label)
        .map_err(|err| format!("Failed to set the Windows save-file picker button label: {err}"))?;
    let (file_type_description, file_type_pattern) =
        save_file_type_spec(file_type_label, extension);
    dialog
        .SetFileTypes(&[(file_type_description.as_str(), file_type_pattern.as_str())])
        .map_err(|err| format!("Failed to set the Windows save-file types: {err}"))?;
    dialog
        .SetFileTypeIndex(1)
        .map_err(|err| format!("Failed to select the Windows save-file type: {err}"))?;
    dialog
        .SetFileName(initial_name)
        .map_err(|err| format!("Failed to set the Windows save-file name: {err}"))?;
    dialog
        .SetDefaultExtension(extension)
        .map_err(|err| format!("Failed to set the Windows save-file extension: {err}"))?;

    let owner = w::HWND::GetDesktopWindow();
    let accepted = dialog
        .Show(&owner)
        .map_err(|err| format!("Failed to show the Windows save-file picker: {err}"))?;
    if !accepted {
        return Ok(None);
    }

    dialog
        .GetResult()
        .and_then(|item| item.GetDisplayName(co::SIGDN::FILESYSPATH))
        .map(Some)
        .map_err(|err| format!("Failed to read the selected Windows save path: {err}"))
}

#[cfg(target_os = "linux")]
pub fn choose_file_bytes(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    overlay: &ToastOverlay,
    log_context: &'static str,
    read_error_message: &'static str,
    on_selected: impl Fn(Vec<u8>) + 'static,
) {
    let dialog = FileDialog::builder()
        .title(gettext(title))
        .accept_label(gettext(accept_label))
        .modal(true)
        .build();
    let overlay = overlay.clone();
    dialog.open(
        Some(window),
        None::<&gio::Cancellable>,
        move |result| match result {
            Ok(file) => match file.load_bytes(None::<&gio::Cancellable>) {
                Ok((bytes, _)) => on_selected(bytes.as_ref().to_vec()),
                Err(err) => {
                    log_error(format!("{log_context}: {err}"));
                    overlay.add_toast(Toast::new(&gettext(read_error_message)));
                }
            },
            Err(err) if err.matches(gio::IOErrorEnum::Cancelled) => {}
            Err(err) => {
                log_error(format!("Failed to open the file chooser: {err}"));
                overlay.add_toast(Toast::new(&gettext(
                    LocalPathKind::File.chooser_error_message(),
                )));
            }
        },
    );
}

#[cfg(target_os = "windows")]
pub fn choose_file_bytes(
    window: &ApplicationWindow,
    title: &str,
    accept_label: &str,
    overlay: &ToastOverlay,
    log_context: &'static str,
    read_error_message: &'static str,
    on_selected: impl Fn(Vec<u8>) + 'static,
) {
    let _ = window;
    match choose_windows_path(title, accept_label, LocalPathKind::File, false) {
        Ok(Some(path)) => match std::fs::read(&path) {
            Ok(bytes) => on_selected(bytes),
            Err(err) => {
                log_error(format!("{log_context}: {err}"));
                overlay.add_toast(Toast::new(&gettext(read_error_message)));
            }
        },
        Ok(None) => {}
        Err(err) => {
            log_error(err);
            overlay.add_toast(Toast::new(&gettext(
                LocalPathKind::File.chooser_error_message(),
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{save_file_type_spec, LocalPathKind};

    #[test]
    fn save_file_type_spec_includes_label_and_extension() {
        assert_eq!(
            save_file_type_spec("CSV files", "csv"),
            ("CSV files (*.csv)".to_string(), "*.csv".to_string())
        );
    }

    #[test]
    fn local_path_messages_match_the_selection_kind() {
        assert_eq!(
            LocalPathKind::File.chooser_error_message(),
            "Couldn't open the file chooser."
        );
        assert_eq!(
            LocalPathKind::Folder.chooser_error_message(),
            "Couldn't open the folder chooser."
        );
        assert_eq!(
            LocalPathKind::File.local_path_error_message(),
            "Choose a local file."
        );
        assert_eq!(
            LocalPathKind::Folder.local_path_error_message(),
            "Choose a local folder."
        );
    }
}
