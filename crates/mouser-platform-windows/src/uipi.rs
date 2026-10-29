//! UIPI (W3): `SendInput` only reaches windows of equal or lower integrity
//! and fails without a sound otherwise. This reads integrity levels so that
//! Mouser can tell when the foreground window belongs to a process above its
//! own: the capture watchdog sends no probe then, since it would vanish.

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Security::{
    GetSidSubAuthority, GetSidSubAuthorityCount, GetTokenInformation, TOKEN_MANDATORY_LABEL,
    TOKEN_QUERY, TokenIntegrityLevel,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

/// The integrity level (the SID's last sub-authority: 0x2000 medium, 0x3000
/// high, 0x4000 system) of process `pid`; `None` when it cannot be opened.
pub(crate) fn integrity_level(pid: u32) -> Option<u32> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut token = HANDLE::default();
    let opened = unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) };
    unsafe { CloseHandle(process).expect("close process") };
    opened.ok()?;
    // Aligned for the TOKEN_MANDATORY_LABEL the system writes into it.
    let mut buf = [0u64; 64];
    let mut len = 0;
    let got = unsafe {
        GetTokenInformation(
            token,
            TokenIntegrityLevel,
            Some(buf.as_mut_ptr().cast()),
            size_of_val(&buf) as u32,
            &mut len,
        )
    };
    unsafe { CloseHandle(token).expect("close token") };
    got.ok()?;
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_MANDATORY_LABEL>()).Label.Sid };
    let count = unsafe { *GetSidSubAuthorityCount(sid) };
    Some(unsafe { *GetSidSubAuthority(sid, u32::from(count) - 1) })
}

/// Whether process `pid` outranks `own`. A process whose level cannot be read
/// is protected from this one, which is the same thing.
pub(crate) fn outranks(pid: u32, own: u32) -> bool {
    integrity_level(pid).is_none_or(|level| level > own)
}

pub(crate) fn foreground_elevated() -> bool {
    window_elevated(unsafe { GetForegroundWindow() })
}

/// Whether the process owning `window` outranks this one; a null window (no
/// foreground window) does not.
pub(crate) fn window_elevated(window: HWND) -> bool {
    window_outranks(window, integrity_level(unsafe { GetCurrentProcessId() }))
}

/// Whether the process owning `window` outranks a process at level `own`; a
/// null window does not. With `own` unknown the window counts as outranking:
/// Mouser cannot tell that its input would reach it.
pub(crate) fn window_outranks(window: HWND, own: Option<u32>) -> bool {
    if window.0.is_null() {
        return false;
    }
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    own.is_none_or(|own| outranks(pid, own))
}
