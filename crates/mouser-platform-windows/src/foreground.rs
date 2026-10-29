//! The application in front (contracts/platform-api-delta.md): its executable
//! file name (FR-003: applications are identified by name only), whether it
//! runs as administrator, and whether it outranks Mouser, so that UIPI drops
//! what Mouser sends it.

use crate::uipi::{integrity_level, window_outranks};
use mouser_platform::ForegroundApp;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
use windows::core::PWSTR;

/// SECURITY_MANDATORY_HIGH_RID: an elevated process. SYSTEM (0x4000) is
/// above it.
const HIGH: u32 = 0x3000;

/// The application owning the foreground window, judged against Mouser's own
/// integrity level.
pub(crate) fn foreground_app() -> Option<ForegroundApp> {
    let own = integrity_level(unsafe { GetCurrentProcessId() });
    app_of_window(unsafe { GetForegroundWindow() }, own)
}

/// The application owning `window`, judged against Mouser at level `own`;
/// `None` for a null window (nothing in front), or when the process that
/// owned the window has exited.
pub(crate) fn app_of_window(window: HWND, own: Option<u32>) -> Option<ForegroundApp> {
    if window.0.is_null() {
        return None;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    let name = name_of(pid, opened_name)?;
    Some(ForegroundApp {
        name,
        admin: is_admin(integrity_level(pid)),
        outranks_mouser: window_outranks(window, own),
    })
}

/// Whether a process at `level` runs as administrator: elevated or as
/// SYSTEM. One whose level cannot be read counts as one, as it is most
/// likely a SYSTEM process.
pub(crate) fn is_admin(level: Option<u32>) -> bool {
    level.is_none_or(|level| level >= HIGH)
}

/// The executable file name of process `pid`: read through a handle to the
/// process (`opened`) or, when it cannot be opened, from a process snapshot.
/// The snapshot is the documented fallback for processes Mouser may not
/// open: most SYSTEM processes refuse PROCESS_QUERY_LIMITED_INFORMATION to a
/// Mouser that is not elevated (probe P1). `None` when the process is in
/// neither, because it has exited.
pub(crate) fn name_of(pid: u32, opened: impl Fn(u32) -> Option<String>) -> Option<String> {
    opened(pid).or_else(|| snapshot_name(pid))
}

/// The executable file name of process `pid`, read through a handle to it.
pub(crate) fn opened_name(pid: u32) -> Option<String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut path = [0u16; 1024];
    let mut len = path.len() as u32;
    let name = PWSTR(path.as_mut_ptr());
    let got = unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, name, &mut len) };
    let _ = unsafe { CloseHandle(process) };
    got.ok()?;
    let path = std::path::PathBuf::from(String::from_utf16_lossy(&path[..len as usize]));
    Some(path.file_name()?.to_string_lossy().into_owned())
}

/// The executable file name a process snapshot lists for `pid`.
fn snapshot_name(pid: u32) -> Option<String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut name = None;
    let mut listed = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while listed && name.is_none() {
        if entry.th32ProcessID == pid {
            let file = &entry.szExeFile;
            let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
            name = Some(String::from_utf16_lossy(&file[..len]));
        }
        listed = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    let _ = unsafe { CloseHandle(snapshot) };
    name
}
