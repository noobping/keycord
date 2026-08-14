use keycord_architecture::validate_workspace;
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = match workspace_root_from_args() {
        Ok(root) => root,
        Err(message) => {
            eprintln!("architecture check: {message}");
            return ExitCode::FAILURE;
        }
    };

    match validate_workspace(&root) {
        Ok(report) => {
            println!(
                "architecture check passed: {} crates, {} UI fragments, {} owned window actions, {} production Rust files; {} allowed legacy catchall files remain",
                report.subject_crates,
                report.ui_fragments,
                report.window_actions,
                report.production_rust_files,
                report.legacy_catchall_files,
            );
            ExitCode::SUCCESS
        }
        Err(violations) => {
            eprintln!(
                "architecture check failed with {} violation(s):",
                violations.len()
            );
            for violation in violations {
                eprintln!("- {violation}");
            }
            ExitCode::FAILURE
        }
    }
}

fn workspace_root_from_args() -> Result<PathBuf, String> {
    let mut args = env::args_os().skip(1);
    let root = if let Some(path) = args.next() {
        PathBuf::from(path)
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .ok_or_else(|| "tool must live below the workspace root".to_string())?
            .to_path_buf()
    };

    if args.next().is_some() {
        return Err("usage: keycord-architecture [WORKSPACE_ROOT]".to_string());
    }
    Ok(root)
}
