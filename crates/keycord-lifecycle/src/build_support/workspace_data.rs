//! Lossless aggregation of crate-owned build data.

use std::fs;
use std::io;
use std::path::Path;

pub(super) fn merge_workspace_data(source_root: &Path) -> io::Result<()> {
    let crates_dir = source_root.join("crates");
    let destination_root = source_root.join("data");
    fs::create_dir_all(&destination_root)?;

    for crate_dir in sorted_directory_entries(&crates_dir)? {
        if !crate_dir.file_type()?.is_dir() {
            continue;
        }

        let source_data = crate_dir.path().join("data");
        match fs::symlink_metadata(&source_data) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => return Err(unsupported_source(&source_data)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }

        let destination_data = destination_root.join(crate_dir.file_name());
        merge_directory(&source_data, &destination_data)?;
    }

    Ok(())
}

fn merge_directory(source: &Path, destination: &Path) -> io::Result<()> {
    ensure_directory(destination)?;

    for entry in sorted_directory_entries(source)? {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            merge_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            copy_file_if_changed(&source_path, &destination_path)?;
        } else {
            return Err(unsupported_source(&source_path));
        }
    }

    Ok(())
}

fn sorted_directory_entries(directory: &Path) -> io::Result<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    Ok(entries)
}

fn ensure_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => Ok(()),
        Ok(_) => Err(path_conflict(path, "directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::create_dir_all(path),
        Err(error) => Err(error),
    }
}

fn copy_file_if_changed(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::symlink_metadata(destination) {
        Ok(metadata) if !metadata.file_type().is_file() => {
            return Err(path_conflict(destination, "file"));
        }
        Ok(_) => {
            if fs::read(source)? == fs::read(destination)? {
                return Ok(());
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    fs::copy(source, destination)?;
    Ok(())
}

fn unsupported_source(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "workspace data source must be a regular file or directory: {}",
            path.display()
        ),
    )
}

fn path_conflict(path: &Path, expected: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!(
            "workspace data destination is not a {expected}: {}",
            path.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::merge_workspace_data;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("keycord-{label}-{}-{unique}", std::process::id()))
    }

    fn write(path: impl AsRef<Path>, contents: &[u8]) {
        let path = path.as_ref();
        fs::create_dir_all(path.parent().expect("test file should have a parent"))
            .expect("create test parent");
        fs::write(path, contents).expect("write test file");
    }

    #[test]
    fn crate_data_is_merged_recursively_under_owner_namespaces() {
        let root = temporary_directory("workspace-data-recursive");
        write(root.join("crates/keycord-alpha/data/alpha.txt"), b"alpha");
        write(
            root.join("crates/keycord-alpha/data/symbolic/apps/alpha.svg"),
            b"alpha icon",
        );
        write(
            root.join("crates/keycord-beta/data/symbolic/apps/beta.svg"),
            b"beta icon",
        );

        merge_workspace_data(&root).expect("merge workspace data");

        assert_eq!(
            fs::read(root.join("data/keycord-alpha/alpha.txt")).unwrap(),
            b"alpha"
        );
        assert_eq!(
            fs::read(root.join("data/keycord-alpha/symbolic/apps/alpha.svg")).unwrap(),
            b"alpha icon"
        );
        assert_eq!(
            fs::read(root.join("data/keycord-beta/symbolic/apps/beta.svg")).unwrap(),
            b"beta icon"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn same_relative_paths_from_different_crates_are_preserved() {
        let root = temporary_directory("workspace-data-collisions");
        write(
            root.join("crates/keycord-alpha/data/window-pages.fragment.ui"),
            b"alpha page",
        );
        write(
            root.join("crates/keycord-beta/data/window-pages.fragment.ui"),
            b"beta page",
        );

        merge_workspace_data(&root).expect("merge workspace data");

        assert_eq!(
            fs::read(root.join("data/keycord-alpha/window-pages.fragment.ui")).unwrap(),
            b"alpha page"
        );
        assert_eq!(
            fs::read(root.join("data/keycord-beta/window-pages.fragment.ui")).unwrap(),
            b"beta page"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rerunning_updates_sources_and_preserves_destination_only_files() {
        let root = temporary_directory("workspace-data-rerun");
        let source = root.join("crates/keycord-alpha/data/value.txt");
        write(&source, b"first");
        write(root.join("data/resources.xml"), b"root owned");
        write(
            root.join("data/keycord-alpha/local.txt"),
            b"destination only",
        );

        merge_workspace_data(&root).expect("first workspace data merge");
        write(&source, b"second");
        merge_workspace_data(&root).expect("second workspace data merge");

        assert_eq!(
            fs::read(root.join("data/keycord-alpha/value.txt")).unwrap(),
            b"second"
        );
        assert_eq!(
            fs::read(root.join("data/keycord-alpha/local.txt")).unwrap(),
            b"destination only"
        );
        assert_eq!(
            fs::read(root.join("data/resources.xml")).unwrap(),
            b"root owned"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_directory_destination_conflicts_are_rejected() {
        let root = temporary_directory("workspace-data-conflict");
        write(
            root.join("crates/keycord-alpha/data/symbolic/apps/icon.svg"),
            b"icon",
        );
        write(root.join("data/keycord-alpha/symbolic"), b"not a directory");

        let error = merge_workspace_data(&root).expect_err("destination conflict should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(error.to_string().contains("keycord-alpha/symbolic"));

        let _ = fs::remove_dir_all(root);
    }
}
