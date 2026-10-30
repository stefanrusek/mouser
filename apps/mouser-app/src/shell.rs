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
    fn replace(&self, bindings: &[(String, Hotkey)], press: Press) -> Vec<(Hotkey, String)> {
        let shortcuts = self.0.global_shortcut();
        shortcuts
            .unregister_all()
            .expect("old hotkeys can be released");
        let mut refused = Vec::new();
        for (accelerator, hotkey) in bindings {
            let (hotkey, press) = (*hotkey, press.clone());
            let registered =
                shortcuts.on_shortcut(accelerator.as_str(), move |_app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        press(hotkey);
                    }
                });
            // The system may refuse a key another application holds (FR-002).
            if let Err(e) = registered {
                refused.push((
                    hotkey,
                    format!("{accelerator} could not be registered: {e}"),
                ));
            }
        }
        refused
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

/// Run Mouser headless (no window, no tray) until it is told to stop.
pub fn run_headless() {
    let args: Vec<String> = std::env::args().collect();
    let os = platform_select::current_os();
    let paths = mouser_core::paths::Paths::resolve(
        &std::env::current_exe().expect("the executable has a path"),
        os,
        &mouser_core::paths::Env::from_process(),
    );
    let mode = crate::mode::select(&args, false, false);
    #[cfg(target_os = "windows")]
    if mode == mouser_proto::wire::ComputerMode::Service {
        run_session_engine(os, &paths);
        return;
    }
    tauri::async_runtime::block_on(async {
        crate::headless::run(
            crate::headless::params(
                platform_select::create(),
                os,
                crate::headless::data_dir(os, mode, &paths),
                mouser_core::control::default_path(),
                gethostname::gethostname()
                    .into_string()
                    .expect("computer name is valid UTF-8"),
                env!("CARGO_PKG_VERSION").to_string(),
                mode,
            ),
            crate::headless::stop_signal(),
        )
        .await;
    });
}

/// The session engine (Windows `--headless --machine`, research V9): it
/// connects to `MouserService` first, sends Ctrl-Alt-Del through it, and
/// runs until the system or the service says to stop.
#[cfg(target_os = "windows")]
fn run_session_engine(os: mouser_platform::Os, paths: &mouser_core::paths::Paths) {
    use mouser_platform_windows::service_client::ServiceClient;
    let version = env!("CARGO_PKG_VERSION");
    let mode = mouser_proto::wire::ComputerMode::Service;
    let client = ServiceClient::connect(version).expect("MouserService made the engine's pipe");
    let platform = Arc::new(mouser_platform_windows::WinPlatform::with_service_client(
        mouser_platform_windows::Settings::system(),
        client,
    ));
    let params = crate::headless::params(
        platform.clone(),
        os,
        crate::headless::data_dir(os, mode, paths),
        mouser_core::control::default_path(),
        gethostname::gethostname()
            .into_string()
            .expect("computer name is valid UTF-8"),
        version.to_string(),
        mode,
    );
    tauri::async_runtime::block_on(crate::headless::serve(
        params,
        Arc::new(OnService(platform)),
        crate::headless::stop_signal(),
    ));
}

/// `MouserService` through the client the backend was built with.
#[cfg(target_os = "windows")]
struct OnService(Arc<mouser_platform_windows::WinPlatform>);

#[cfg(target_os = "windows")]
impl OnService {
    fn client(&self) -> &mouser_platform_windows::service_client::ServiceClient {
        self.0
            .service_client()
            .expect("built with the service client")
    }
}

#[cfg(target_os = "windows")]
impl crate::headless::Service for OnService {
    fn on_shutdown(&self, f: Box<dyn FnOnce() + Send>) {
        self.client().on_shutdown(f);
    }

    fn stopping(&self, reason: mouser_proto::service_ipc::StopReason) {
        // A service that has gone has nothing to hear.
        let _ = self.client().stopping(reason);
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
            commands::hotkey_refusals,
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
                // Core acts on QuickJump and targeted paste from the captured
                // stream; the registration only keeps them from the
                // application in front, so the press is only reported.
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
