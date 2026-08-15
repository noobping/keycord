//! Top-level application build pipeline.

use super::{
    app_id, desktop_file, metadata, resources, search_provider_bus_name,
    search_provider_object_path, translations, workspace_data, write_install_assets,
    ApplicationBuildConfig, GETTEXT_DOMAIN, RESOURCE_ID,
};

pub fn run_application_build(config: &ApplicationBuildConfig<'_>) {
    let app_id = app_id(config.debug, config.flatpak);
    let search_provider_bus_name = search_provider_bus_name(app_id);
    let search_provider_object_path = search_provider_object_path(&search_provider_bus_name);

    println!("cargo:rustc-env=APP_ID={app_id}");
    println!("cargo:rustc-env=RESOURCE_ID={RESOURCE_ID}");
    println!("cargo:rustc-env=GETTEXT_DOMAIN={GETTEXT_DOMAIN}");
    println!("cargo:rustc-env=SEARCH_PROVIDER_BUS_NAME={search_provider_bus_name}");
    println!("cargo:rustc-env=SEARCH_PROVIDER_OBJECT_PATH={search_provider_object_path}");

    metadata::export_dependency_versions(config.source_root);
    resources::write_window_ui(config.source_root, config.out_dir, GETTEXT_DOMAIN);

    #[cfg(target_os = "windows")]
    super::windows::configure_binary_stack_size(config.target_os, config.target_env);

    resources::build_resources(config.source_root, RESOURCE_ID);

    #[cfg(target_os = "windows")]
    super::windows::compile_binary_resource(config.source_root, config.package_name);

    let desktop_entry = desktop_file(
        app_id,
        config.package_name,
        config.display_name,
        config.package_description,
        config.passkey_mime,
    );
    let locales = translations::build_catalogs(
        config.source_root,
        config.out_dir,
        GETTEXT_DOMAIN,
        config.package_name,
        config.package_version,
        &desktop_entry,
    );
    println!(
        "cargo:rustc-env=LOCALEDIR={}",
        config.out_dir.join("locale").display()
    );
    println!("cargo:rustc-env=AVAILABLE_LOCALES={}", locales.join(":"));

    if !config.setup {
        write_install_assets(
            &config.source_root.join("crates/keycord-lifecycle/data"),
            app_id,
            config.package_name,
            config.display_name,
            config.package_description,
            config.passkey_mime,
        )
        .expect("Can not build lifecycle install assets");
    }

    workspace_data::merge_workspace_data(config.source_root)
        .expect("Can not merge workspace data into the root data directory");

    emit_rerun_directives();
}

fn emit_rerun_directives() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=data");
    println!("cargo:rerun-if-changed=po");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=crates");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_SETUP");
}
