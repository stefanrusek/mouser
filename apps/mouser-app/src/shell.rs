//! The live app shell: the real tray icon, plugins and event loop. This is
//! the only code that runs solely inside a launched app; everything it calls
//! is unit tested through Tauri's mock runtime.

use std::sync::Arc;

use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, RunEvent, Runtime, WindowEvent};
use tauri_plugin_notification::NotificationExt;

use crate::host::Host;
use crate::notify::Notifier;
use crate::tray::{self, TrayController, TrayPort, TrayView};
use crate::{app, commands, host, platform_select};

struct LiveTray<R: Runtime> {
    app: AppHandle<R>,
    icon: TrayIcon<R>,
}

impl<R: Runtime> TrayPort for LiveTray<R> {
    fn apply(&self, view: &TrayView) {
        let items: Vec<MenuItem<R>> = view
            .entries
            .iter()
            .map(|e| {
                MenuItem::with_id(
                    &self.app,
                    e.id.unwrap_or("summary"),
                    &e.label,
                    e.id.is_some(),
                    None::<&str>,
                )
                .expect("menu item can be created")
            })
            .collect();
        let refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = items.iter().map(|i| i as _).collect();
        let menu = Menu::with_items(&self.app, &refs).expect("menu can be created");
        let image = Image::from_bytes(view.icon.png()).expect("tray icon is a PNG");
        self.icon
            .set_icon(Some(image))
            .expect("tray icon can be set");
        self.icon
            .set_icon_as_template(view.icon.is_template())
            .expect("tray template flag can be set");
        self.icon
            .set_tooltip(Some(&view.summary))
            .expect("tray tooltip can be set");
        self.icon
            .set_menu(Some(menu))
            .expect("tray menu can be set");
    }
}

struct LiveNotifier<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Notifier for LiveNotifier<R> {
    fn show(&self, title: &str, body: &str) {
        self.0
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
            .expect("system notification can be shown");
    }
}

/// Run Mouser until the owner quits it.
pub fn run() {
    tauri::Builder::default()
        // Single instance must be registered first: a second launch opens
        // the first one's window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            host::on_second_launch(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            commands::control,
            commands::open_permission_settings,
            commands::fix,
            commands::hide_window
        ])
        .setup(|app| {
            let host = tauri::async_runtime::block_on(Host::start(
                platform_select::create(),
                mouser_core::config::default_dir(),
                mouser_core::control::default_path(),
                gethostname::gethostname()
                    .into_string()
                    .expect("computer name is valid UTF-8"),
                env!("CARGO_PKG_VERSION").to_string(),
            ));
            let control = host.control();
            app.manage(host);
            let handle = app.handle().clone();
            let appearance = tray::system_appearance(&handle);
            let menu_control = control.clone();
            let icon = TrayIconBuilder::with_id("mouser")
                .show_menu_on_left_click(true)
                .on_menu_event(move |app, event| match tray::action(event.id.as_ref()) {
                    tray::Action::Open => tray::open(app, &menu_control),
                    tray::Action::Quit => app.exit(0),
                })
                .build(app)?;
            let controller = Arc::new(TrayController::new(
                Box::new(LiveTray {
                    app: handle.clone(),
                    icon,
                }),
                platform_select::current_os(),
                appearance,
            ));
            let theme_controller = controller.clone();
            app.get_webview_window("main")
                .expect("main window exists")
                .on_window_event(move |event| {
                    if let WindowEvent::ThemeChanged(theme) = event {
                        theme_controller.update_appearance(tray::appearance_of(*theme));
                    }
                });
            tauri::async_runtime::spawn(async move {
                let notes = app::start(&handle, &control, &controller).await;
                app::pump(
                    handle.clone(),
                    controller,
                    Arc::new(LiveNotifier(handle)),
                    notes,
                )
                .await;
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Mouser can start")
        .run(|_app, event| {
            // Closing the last window leaves Mouser running in the tray.
            if let RunEvent::ExitRequested {
                api, code: None, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
