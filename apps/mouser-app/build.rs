fn main() {
    // The default Tauri manifest is replaced so the app is Per-Monitor-V2 DPI
    // aware (cursor and screen edges stay exact across mixed-DPI monitors).
    // Common-Controls v6 stays declared because Tauri's dialogs need it.
    let windows =
        tauri_build::WindowsAttributes::new().app_manifest(include_str!("windows/mouser.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("tauri build script failed");
}
