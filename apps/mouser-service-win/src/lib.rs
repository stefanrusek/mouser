//! `MouserService` (research V9): the LocalSystem supervisor that starts the
//! session engine in the console session, answers its Ctrl-Alt-Del requests,
//! and installs and uninstalls Mouser's machine-wide parts.

pub mod winapi;
#[cfg(windows)]
pub mod winapi_sys;
