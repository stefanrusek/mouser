//! Mouser's desktop app: a Tauri tray app hosting the engine and the UI.

pub mod app;
pub mod commands;
pub mod host;
pub mod notify;
pub mod platform_select;
pub mod tray;
pub mod windows;

#[cfg(feature = "gui")]
pub mod shell;

#[cfg(test)]
mod app_tests;
#[cfg(test)]
mod commands_tests;
#[cfg(test)]
mod host_tests;
#[cfg(test)]
mod notify_tests;
#[cfg(test)]
mod platform_select_tests;
#[cfg(test)]
mod support_tests;
#[cfg(test)]
mod tray_model_tests;
#[cfg(test)]
mod windows_tests;
