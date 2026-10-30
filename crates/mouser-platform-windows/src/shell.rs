//! What the app asks of Windows around starting and restarting itself
//! (FR-005; research V2; probe P3): restarting as administrator through the
//! UAC prompt, waiting for the instance it replaces, telling whether an
//! elevated instance is still running, and saying so in a message box; and
//! whether `MouserService` runs, which puts the tray app in UI mode.

use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, WAIT_TIMEOUT};
use windows::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};
use windows::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MessageBoxW, SW_SHOWNORMAL,
};
use windows::core::{HSTRING, PCWSTR, w};

/// Why a program could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunAs {
    /// The user said no at the UAC prompt.
    Declined,
    /// Windows refused, for this reason.
    Failed(String),
}

/// What a failure of `ShellExecuteExW` means.
pub(crate) fn outcome(e: &windows::core::Error) -> RunAs {
    if e.code() == ERROR_CANCELLED.to_hresult() {
        RunAs::Declined
    } else {
        RunAs::Failed(e.message())
    }
}

/// Starts `exe` with `args` through the shell with `verb`.
pub(crate) fn shell_execute(verb: &str, exe: &Path, args: &str) -> Result<(), RunAs> {
    let (verb, exe, args) = (
        HSTRING::from(verb),
        HSTRING::from(exe.as_os_str()),
        HSTRING::from(args),
    );
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        // Errors come back here, not as the shell's own message box.
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(args.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| outcome(&e))?;
    // The started program runs on by itself; its handle is not needed.
    if !info.hProcess.is_invalid() {
        let _ = unsafe { CloseHandle(info.hProcess) };
    }
    Ok(())
}

/// Starts `exe` with `args`; `as_admin` runs it as administrator, which
/// shows the UAC prompt.
pub fn start(exe: &Path, args: &str, as_admin: bool) -> Result<(), RunAs> {
    shell_execute(if as_admin { "runas" } else { "open" }, exe, args)
}

/// Waits until process `pid` has ended; at once when it already has.
/// Only its end counts: unlike the cursor guard's wait, nothing it sets
/// first does.
pub fn wait_for_process(pid: u32) {
    if let Ok(process) = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) } {
        unsafe { WaitForSingleObject(process, INFINITE) };
        let _ = unsafe { CloseHandle(process) };
    }
}

/// Whether process `pid` is still running.
pub fn process_alive(pid: u32) -> bool {
    let Ok(process) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return false;
    };
    let running = unsafe { WaitForSingleObject(process, 0) } == WAIT_TIMEOUT;
    let _ = unsafe { CloseHandle(process) };
    running
}

/// Whether this process runs as administrator.
pub fn this_process_elevated() -> bool {
    crate::foreground::is_admin(crate::uipi::integrity_level(std::process::id()))
}

/// The SID of the LocalSystem account.
pub const LOCAL_SYSTEM: &str = "S-1-5-18";

/// Whether this process runs as LocalSystem, as MouserService and its
/// session engine do.
pub fn this_process_is_local_system() -> bool {
    crate::secrets::current_user_sid() == LOCAL_SYSTEM
}

/// Shows `text` in a message box titled "Mouser" and waits until it is
/// closed.
pub fn message_box(text: &str) {
    let flags = MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND;
    unsafe { MessageBoxW(None, &HSTRING::from(text), w!("Mouser"), flags) };
}

/// Whether the Windows service `name` is installed and running.
pub fn service_running(name: &str) -> bool {
    use windows::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, QueryServiceStatus, SC_MANAGER_CONNECT,
        SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS,
    };
    unsafe { OpenSCManagerW(None, None, SC_MANAGER_CONNECT) }.is_ok_and(|manager| {
        let mut status = SERVICE_STATUS::default();
        let running = unsafe { OpenServiceW(manager, &HSTRING::from(name), SERVICE_QUERY_STATUS) }
            .is_ok_and(|service| {
                let queried = unsafe { QueryServiceStatus(service, &mut status) };
                let _ = unsafe { CloseServiceHandle(service) };
                queried.is_ok() && status.dwCurrentState == SERVICE_RUNNING
            });
        let _ = unsafe { CloseServiceHandle(manager) };
        running
    })
}
