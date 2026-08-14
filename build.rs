use keycord_lifecycle::build_support::{
    run_application_build, ApplicationBuildConfig, PasskeyMimeConfig,
};
use std::env;
use std::path::PathBuf;

fn main() {
    let source_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR not set for build script"));
    let target_os = env::var("CARGO_CFG_TARGET_OS").ok();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").ok();
    let config = ApplicationBuildConfig {
        source_root: &source_root,
        out_dir: &out_dir,
        package_name: env!("CARGO_PKG_NAME"),
        package_version: env!("CARGO_PKG_VERSION"),
        package_description: option_env!("CARGO_PKG_DESCRIPTION").unwrap_or("Password manager"),
        display_name: "Keycord",
        debug: cfg!(debug_assertions),
        flatpak: cfg!(feature = "flatpak"),
        passkey_mime: cfg!(feature = "passkey").then_some(PasskeyMimeConfig {
            mime_types: keycord_passkey::PASSKEY_MIME_TYPES,
            package: keycord_passkey::PASSKEY_MIME_PACKAGE,
        }),
        setup: cfg!(feature = "setup"),
        target_os: target_os.as_deref(),
        target_env: target_env.as_deref(),
    };
    run_application_build(&config);
}
