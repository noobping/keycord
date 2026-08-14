//! Entry-specific application launch requests.

use crate::model::OpenPassFile;
use std::ffi::OsString;

/// Parses the stable `--open-entry STORE LABEL` command-line contract.
pub fn command_line_open_entry(args: &[OsString]) -> Option<OpenPassFile> {
    if args.get(1).is_none_or(|arg| arg != "--open-entry") {
        return None;
    }

    let store_root = args.get(2)?.to_string_lossy().into_owned();
    let label = args.get(3)?.to_string_lossy().into_owned();
    if store_root.is_empty() || label.is_empty() {
        return None;
    }

    Some(OpenPassFile::from_label(store_root, label))
}

#[cfg(test)]
mod tests {
    use super::command_line_open_entry;
    use std::ffi::OsString;

    #[test]
    fn open_entry_command_line_is_parsed() {
        let args = vec![
            OsString::from("keycord"),
            OsString::from("--open-entry"),
            OsString::from("/tmp/store"),
            OsString::from("work/alice/github"),
        ];

        let pass_file = command_line_open_entry(&args).expect("expected pass file");
        assert_eq!(pass_file.store_path(), "/tmp/store");
        assert_eq!(pass_file.label(), "work/alice/github".to_string());
    }

    #[test]
    fn unrelated_or_incomplete_arguments_are_ignored() {
        assert!(
            command_line_open_entry(&[OsString::from("keycord"), OsString::from("query"),])
                .is_none()
        );
        assert!(command_line_open_entry(&[
            OsString::from("keycord"),
            OsString::from("--open-entry"),
            OsString::from("/tmp/store"),
        ])
        .is_none());
    }
}
