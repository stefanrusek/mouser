//! Whether the foreground app is full screen, so edge crossing can be blocked
//! (`crossing.block_fullscreen`).
//!
//! Full screen means: `SHQueryUserNotificationState` says the user is in a
//! full-screen app (`QUNS_BUSY`) or a Direct3D full-screen game
//! (`QUNS_RUNNING_D3D_FULL_SCREEN`), or the foreground window has no title
//! bar and covers its whole monitor rectangle. The desktop and taskbar cover
//! their monitor too and never count; a maximized window keeps its title bar,
//! so an auto-hidden taskbar does not make it "full screen".

use crate::dpi;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::UI::Shell::{
    QUERY_USER_NOTIFICATION_STATE, QUNS_BUSY, QUNS_RUNNING_D3D_FULL_SCREEN,
    SHQueryUserNotificationState,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, WS_CAPTION,
};

/// What Windows says about the foreground window.
pub(crate) struct Foreground {
    pub(crate) window: RECT,
    pub(crate) monitor: RECT,
    pub(crate) class: String,
    pub(crate) style: u32,
}

/// Window classes of the desktop and the taskbar, which fill a monitor
/// without being a full-screen app.
const SHELL_CLASSES: [&str; 4] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
];

pub(crate) fn state_means_fullscreen(state: QUERY_USER_NOTIFICATION_STATE) -> bool {
    state == QUNS_BUSY || state == QUNS_RUNNING_D3D_FULL_SCREEN
}

fn covers(window: &RECT, monitor: &RECT) -> bool {
    window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

pub(crate) fn window_is_fullscreen(f: &Foreground) -> bool {
    !SHELL_CLASSES.contains(&f.class.as_str())
        && f.style & WS_CAPTION.0 != WS_CAPTION.0
        && covers(&f.window, &f.monitor)
}

/// Both signals; `foreground` is `None` when there is no foreground window.
pub(crate) fn decide(state: Option<QUERY_USER_NOTIFICATION_STATE>, f: Option<&Foreground>) -> bool {
    state.is_some_and(state_means_fullscreen) || f.is_some_and(window_is_fullscreen)
}

pub(crate) fn foreground() -> Option<Foreground> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.0.is_null()).then_some(())?;
    let mut window = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut window) }.ok()?;
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    unsafe { GetMonitorInfoW(monitor, &mut info) }.ok().ok()?;
    let mut name = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut name) }.max(0) as usize;
    Some(Foreground {
        window,
        monitor: info.rcMonitor,
        class: String::from_utf16_lossy(&name[..len]),
        style: unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32,
    })
}

pub(crate) fn foreground_is_fullscreen() -> bool {
    dpi::physical(|| {
        let state = unsafe { SHQueryUserNotificationState() }.ok();
        decide(state, foreground().as_ref())
    })
}
