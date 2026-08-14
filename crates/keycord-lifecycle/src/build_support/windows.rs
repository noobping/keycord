//! Native Windows build-script integration.

use std::path::Path;

const WINDOWS_MAIN_THREAD_STACK_SIZE_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn configure_binary_stack_size(target_os: Option<&str>, target_env: Option<&str>) {
    if target_os != Some("windows") {
        return;
    }

    match target_env {
        Some("gnu") => println!(
            "cargo:rustc-link-arg-bins=-Wl,--stack,{}",
            WINDOWS_MAIN_THREAD_STACK_SIZE_BYTES
        ),
        Some("msvc") => println!(
            "cargo:rustc-link-arg-bins=/STACK:{}",
            WINDOWS_MAIN_THREAD_STACK_SIZE_BYTES
        ),
        _ => {}
    }
}

pub(super) fn compile_binary_resource(source_root: &Path, package_name: &str) {
    let relative_icon_path = format!("crates/keycord-lifecycle/data/branding/{package_name}.ico");
    let icon_path = source_root.join(&relative_icon_path);
    println!("cargo:rerun-if-changed={relative_icon_path}");
    let mut resource = winresource::WindowsResource::new();
    resource.set_icon(icon_path.to_string_lossy().as_ref());
    resource.compile().expect("Failed to compile resources");
}
