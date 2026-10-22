//! Portable mode (Story 13): with a `MouserData/` folder beside the
//! executable, the app keeps its settings, secrets and WebView data there and
//! installs nothing on the computer it runs from.

use std::path::PathBuf;
use std::sync::Arc;

use mouser_core::EngineParams;
use mouser_core::paths::Paths;
use mouser_platform::Platform;
use tauri::{Manager, Runtime, WebviewWindowBuilder};

/// The WebView's data folder: inside `MouserData/` when portable.
pub fn webview_data_dir(paths: &Paths) -> Option<PathBuf> {
    paths.portable.then(|| paths.dir.join("webview"))
}

/// Engine parameters that keep everything in `paths`.
pub fn engine_params(
    platform: Arc<dyn Platform>,
    paths: &Paths,
    control_path: PathBuf,
    computer_name: String,
    version: String,
) -> EngineParams {
    EngineParams::new(
        platform,
        paths.dir.clone(),
        control_path,
        computer_name,
        version,
    )
    .with_paths(paths)
}

/// The platform for this computer, portable when `paths` say so.
#[cfg(target_os = "macos")]
pub fn create_platform(paths: &Paths) -> Arc<dyn Platform> {
    if paths.portable {
        Arc::new(mouser_platform_macos::MacPlatform::portable(
            paths.dir.clone(),
        ))
    } else {
        crate::platform_select::create()
    }
}

/// The platform for this computer, portable when `paths` say so.
#[cfg(target_os = "linux")]
pub fn create_platform(paths: &Paths) -> Arc<dyn Platform> {
    if paths.portable {
        Arc::new(mouser_platform_linux::LinuxPlatform::portable(
            paths.dir.clone(),
        ))
    } else {
        crate::platform_select::create()
    }
}

/// The platform for this computer, portable when `paths` say so.
#[cfg(windows)]
pub fn create_platform(paths: &Paths) -> Arc<dyn Platform> {
    if paths.portable {
        Arc::new(mouser_platform_windows::WinPlatform::portable(
            paths.dir.clone(),
        ))
    } else {
        crate::platform_select::create()
    }
}

/// The platform for this computer.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn create_platform(_paths: &Paths) -> Arc<dyn Platform> {
    crate::platform_select::create()
}

/// Put a window's WebView data in `MouserData/` when this run is portable.
pub fn with_webview_data<'a, R: Runtime, M: Manager<R>>(
    manager: &M,
    builder: WebviewWindowBuilder<'a, R, M>,
) -> WebviewWindowBuilder<'a, R, M> {
    match manager
        .try_state::<Paths>()
        .and_then(|p| webview_data_dir(&p))
    {
        Some(dir) => builder.data_directory(dir),
        None => builder,
    }
}

/// Whether this run is portable (the UI hides what would install anything).
#[tauri::command]
pub fn is_portable(paths: tauri::State<'_, Paths>) -> bool {
    paths.portable
}
