fn main() {
    // Per-Monitor-V2 DPI awareness (cursor and screen edges stay exact across
    // mixed-DPI monitors) and Common-Controls v6 (Tauri's dialogs need it) come
    // from windows/mouser.manifest. tauri-build only embeds its manifest into
    // the bin target, so a lib unit-test or integration-test executable that
    // links Tauri dies at load with STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139).
    // So Tauri's own manifest is dropped and the linker embeds ours into every
    // executable of this package (rustc-link-arg applies to all targets).
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri build script failed");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap();
    if target_os == "windows" && target_env == "msvc" {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("windows")
            .join("mouser.manifest");
        println!("cargo:rerun-if-changed={}", manifest.display());
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    }
}
