//! `MouserService` (research V9): the LocalSystem supervisor that starts the
//! session engine in the console session, answers its Ctrl-Alt-Del requests,
//! and installs and uninstalls Mouser's machine-wide parts.

#[doc(hidden)]
pub mod fake;
pub mod install;
#[cfg(test)]
mod install_tests;
pub mod ipc;
#[cfg(test)]
mod ipc_tests;
pub mod launch;
#[cfg(test)]
mod launch_tests;
pub mod sas;
#[cfg(test)]
mod sas_tests;
pub mod session;
#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod test_log;
pub mod winapi;
#[cfg(windows)]
pub mod winapi_sys;
