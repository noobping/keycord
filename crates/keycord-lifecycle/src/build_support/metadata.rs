//! Cargo lockfile metadata exported to the application at compile time.

use std::fs;
use std::path::Path;

pub(super) fn export_dependency_versions(source_root: &Path) {
    let lockfile = fs::read_to_string(source_root.join("Cargo.lock"))
        .expect("Failed to read Cargo.lock for version metadata");
    let ripasso = find_locked_package_version(&lockfile, "ripasso")
        .expect("ripasso version not found in Cargo.lock");
    let sequoia = find_locked_package_version(&lockfile, "sequoia-openpgp")
        .expect("sequoia-openpgp version not found in Cargo.lock");

    println!("cargo:rustc-env=RIPASSO_VERSION={ripasso}");
    println!("cargo:rustc-env=SEQUOIA_OPENPGP_VERSION={sequoia}");
}

fn find_locked_package_version(lockfile: &str, package: &str) -> Option<String> {
    let mut current_package = None;

    for line in lockfile.lines() {
        let trimmed = line.trim();

        if trimmed == "[[package]]" {
            current_package = None;
            continue;
        }

        if let Some(name) = trimmed
            .strip_prefix("name = \"")
            .and_then(|value| value.strip_suffix('"'))
        {
            current_package = Some(name);
            continue;
        }

        if current_package == Some(package) {
            if let Some(version) = trimmed
                .strip_prefix("version = \"")
                .and_then(|value| value.strip_suffix('"'))
            {
                return Some(version.to_string());
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::find_locked_package_version;

    #[test]
    fn lockfile_versions_are_selected_by_package_name() {
        let lockfile = "\
[[package]]\n\
name = \"alpha\"\n\
version = \"1.2.3\"\n\
\n\
[[package]]\n\
name = \"beta\"\n\
version = \"4.5.6\"\n";

        assert_eq!(
            find_locked_package_version(lockfile, "beta").as_deref(),
            Some("4.5.6")
        );
        assert_eq!(find_locked_package_version(lockfile, "missing"), None);
    }
}
