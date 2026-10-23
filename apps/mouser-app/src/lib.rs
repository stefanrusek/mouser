//! Mouser's desktop app: a Tauri tray app hosting the engine and the UI.

pub mod app;
pub mod commands;
pub mod diagnostics;
pub mod headless;
pub mod host;
pub mod hotkeys;
#[cfg(target_os = "macos")]
pub mod login_screen;
pub mod notify;
pub mod overlay;
pub mod platform_select;
pub mod portable;
pub mod quickjump_windows;
pub mod tray;
pub mod tray_sas;
pub mod updater;
pub mod windows;

#[cfg(feature = "gui")]
pub mod shell;

#[cfg(test)]
mod app_tests;
#[cfg(test)]
pub(crate) mod commands_tests;
#[cfg(test)]
mod diagnostics_tests;
#[cfg(test)]
mod headless_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
pub(crate) mod hotkeys_tests;
#[cfg(all(test, target_os = "macos"))]
mod login_screen_tests;
#[cfg(test)]
mod notify_tests;
#[cfg(test)]
mod overlay_tests;
#[cfg(test)]
mod platform_select_tests;
#[cfg(test)]
mod portable_tests;
#[cfg(test)]
mod quickjump_windows_tests;
#[cfg(test)]
mod support_tests;
#[cfg(test)]
mod tray_model_tests;
#[cfg(test)]
mod tray_sas_tests;
#[cfg(test)]
mod updater_tests;
#[cfg(test)]
mod windows_tests;
