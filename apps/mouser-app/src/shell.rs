//! The live app shell: the real tray icon, plugins and event loop. This is
//! the only code that runs solely inside a launched app; everything it calls
//! is unit tested through Tauri's mock runtime.

use std::sync::Arc;

use tauri::{AppHandle, Emitter, Manager, RunEvent, Runtime, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

use crate::host::Host;
use crate::hotkeys::{Hotkey, Hotkeys, Press, Shortcuts};
use crate::notify::Notifier;
use crate::tray::{self, TrayController};
use crate::updater::{self, Applier};
use crate::{app, commands, host, overlay, platform_select, portable};

pub(crate) struct LiveNotifier<R: Runtime>(pub(crate) AppHandle<R>);

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

    // Desktop notifications cannot report a click, so a notification about
    // something to look at brings the window there when it is shown.
    fn open(&self, route: &str) {
        crate::windows::navigate(&self.0, route);
    }
}

struct LiveShortcuts<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Shortcuts for LiveShortcuts<R> {
    fn replace(&self, bindings: &[(String, Hotkey)], press: Press) {
        let shortcuts = self.0.global_shortcut();
        shortcuts
            .unregister_all()
            .expect("old hotkeys can be released");
        for (accelerator, hotkey) in bindings {
            let (hotkey, press) = (*hotkey, press.clone());
            shortcuts
                .on_shortcut(accelerator.as_str(), move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        press(hotkey);
                    }
                })
                .unwrap_or_else(|e| panic!("hotkey {accelerator} can be registered: {e}"));
        }
    }
}

/// Installs an update through `tauri-plugin-updater`, then restarts Mouser.
struct LiveApplier<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Applier for LiveApplier<R> {
    fn install(&self, endpoint: &str, target: &str, pubkey: &str) -> Result<(), String> {
        let (app, endpoint, target, pubkey) = (
            self.0.clone(),
            endpoint.to_string(),
            target.to_string(),
            pubkey.to_string(),
        );
        // The engine calls the installer from inside the async runtime.
        std::thread::spawn(move || {
            tauri::async_runtime::block_on(async move {
                let update = app
                    .updater_builder()
                    .target(target)
                    .pubkey(pubkey)
                    .endpoints(vec![endpoint.parse().expect("the endpoint is a URL")])
                    .map_err(|e| e.to_string())?
                    .build()
                    .map_err(|e| e.to_string())?
                    .check()
                    .await
                    .map_err(|e| e.to_string())?
                    .ok_or("the updater found no newer version")?;
                update
                    .download_and_install(|_, _| {}, || {})
                    .await
                    .map_err(|e| e.to_string())
            })
        })
        .join()
        .expect("the installer thread ends")?;
        self.0.restart()
    }
}

/// Starts the system file manager as a child process.
struct Spawner;

impl commands::FileManager for Spawner {
    fn open(&self, program: &str, args: &[String]) {
        let mut opener = std::process::Command::new(program)
            .args(args)
            .spawn()
            .unwrap_or_else(|e| panic!("{program} can open the file manager: {e}"));
        // Reap the opener when it is done (explorer exits with 1 even on success).
        std::thread::spawn(move || opener.wait());
    }
}

/// Resolves when the system asks Mouser to stop: Ctrl-C or SIGTERM.
#[cfg(unix)]
async fn stop_signal() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("SIGTERM can be awaited");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

/// Resolves when the system asks Mouser to stop: Ctrl-C, the console closing
/// or the computer shutting down.
#[cfg(windows)]
async fn stop_signal() {
    let mut close = tokio::signal::windows::ctrl_close().expect("console close can be awaited");
    let mut shutdown = tokio::signal::windows::ctrl_shutdown().expect("shutdown can be awaited");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = close.recv() => {}
        _ = shutdown.recv() => {}
    }
}

/// Run Mouser headless (no window, no tray) until it is told to stop.
pub fn run_headless() {
    tauri::async_runtime::block_on(crate::headless::run(
        platform_select::create(),
        crate::headless::machine_dir(platform_select::current_os()),
        mouser_core::control::default_path(),
        gethostname::gethostname()
            .into_string()
            .expect("computer name is valid UTF-8"),
        env!("CARGO_PKG_VERSION").to_string(),
        crate::mode::select(&std::env::args().collect::<Vec<_>>(), false, false),
        stop_signal(),
    ));
}

/// Run Mouser until the owner quits it.
pub fn run() {
    tauri::Builder::default()
        // Single instance must be registered first: a second launch opens
        // the first one's window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            host::on_second_launch(app);
        }))
        .on_window_event(crate::windows::hide_on_close)
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            commands::control,
            commands::open_permission_settings,
            commands::fix,
            commands::transfers,
            commands::take_arranger_edge,
            commands::reveal_in_file_manager,
            commands::collect_logs,
            commands::transfer_resolve,
            commands::hide_window,
            portable::is_portable,
            #[cfg(target_os = "macos")]
            crate::login_screen::allow_login_screen
        ])
        .setup(|app| {
            let paths = mouser_core::paths::Paths::resolve(
                &std::env::current_exe().expect("the executable has a path"),
                platform_select::current_os(),
                &mouser_core::paths::Env::from_process(),
            );
            let mut params = portable::engine_params(
                portable::create_platform(&paths),
                &paths,
                mouser_core::control::default_path(),
                gethostname::gethostname()
                    .into_string()
                    .expect("computer name is valid UTF-8"),
                env!("CARGO_PKG_VERSION").to_string(),
            );
            // Windows token elevation has no detector yet (needs a mouser-platform-windows export).
            params.mode =
                crate::mode::select(&std::env::args().collect::<Vec<_>>(), paths.portable, false);
            params.installer = Some(updater::installer(Arc::new(LiveApplier(
                app.handle().clone(),
            ))));
            params.check_schedule = Some(mouser_core::release::CheckSchedule::DEFAULT);
            let config_dir = paths.dir.clone();
            // Managed first: every window built from here on reads it.
            app.manage(paths);
            let host = tauri::async_runtime::block_on(Host::start_with(params));
            let control = host.control();
            app.manage(host);
            #[cfg(target_os = "macos")]
            app.manage::<Box<dyn crate::login_screen::Admin>>(Box::new(
                crate::login_screen::Osascript(|argv: &[String]| {
                    let out = std::process::Command::new("osascript")
                        .args(argv)
                        .output()
                        .expect("osascript can run");
                    crate::login_screen::Outcome {
                        success: out.status.success(),
                        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                    }
                }),
            ));
            app.manage::<Box<dyn commands::FileManager>>(Box::new(Spawner));
            let handle = app.handle().clone();
            // The first-run window is not created from the config: a portable
            // run must give it its data folder first.
            portable::with_webview_data(
                &handle,
                tauri::WebviewWindowBuilder::from_config(&handle, &handle.config().app.windows[0])?,
            )
            .build()?;
            overlay::create(&handle);
            let forward = handle.clone();
            let hotkeys = Hotkeys::new(
                Box::new(LiveShortcuts(handle.clone())),
                control.clone(),
                Box::new(move || mouser_core::config::load_or_create(&config_dir)),
                // QuickJump and targeted paste are acted on by the engine
                // once those stories land; the window layer only reports them.
                Arc::new(move |hotkey| {
                    forward
                        .emit("mouser://hotkey", hotkey.name())
                        .expect("hotkey press can be announced");
                }),
            );
            hotkeys.sync();
            app.manage(hotkeys);
            let events = control.events();
            let appearance = tray::system_appearance(&handle);
            let menu_control = control.clone();
            let icon = crate::tray_runtime::build(app, menu_control)?;
            let controller = Arc::new(TrayController::new(
                Box::new(crate::tray_runtime::LiveTray {
                    app: handle.clone(),
                    icon,
                }),
                platform_select::current_os(),
                appearance,
            ));
            tauri::async_runtime::spawn(updater::poll(control.clone(), controller.clone()));
            let theme_controller = controller.clone();
            app.get_webview_window("main")
                .expect("main window exists")
                .on_window_event(move |event| {
                    if let WindowEvent::ThemeChanged(theme) = event {
                        theme_controller.update_appearance(tray::appearance_of(*theme));
                    }
                });
            let event_app = handle.clone();
            tauri::async_runtime::spawn(app::pump_events(
                event_app,
                Arc::new(LiveNotifier(handle.clone())),
                events,
            ));
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
        .run(|app, event| match event {
            // Closing the last window leaves Mouser running in the tray.
            RunEvent::ExitRequested {
                api, code: None, ..
            } => api.prevent_exit(),
            RunEvent::Exit => crate::exit::on_exit(app),
            _ => {}
        });
}
