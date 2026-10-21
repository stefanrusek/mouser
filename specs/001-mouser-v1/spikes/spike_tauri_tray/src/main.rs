use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

fn main() {
    tauri::Builder::default()
        // single-instance must be registered first
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Show Mouser", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let icon = Image::from_bytes(include_bytes!("../icons/tray-template.png"))?;
            let _tray = TrayIconBuilder::with_id("mouser")
                .icon(icon)
                .icon_as_template(true) // macOS: auto light/dark tint
                .tooltip("Mouser spike")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, ev| match ev.id.as_ref() {
                    "show" => { if let Some(w) = app.get_webview_window("main") { let _ = w.show(); } }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            let al = app.autolaunch();
            println!("SPIKE: tray built; window visible={:?}; autostart enabled={:?}",
                app.get_webview_window("main").map(|w| w.is_visible()), al.is_enabled());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
