//! Following the foreground window (research V7 (a)): an out-of-context
//! `EVENT_SYSTEM_FOREGROUND` WinEvent hook works out the application in
//! front again each time the foreground changes. Its events arrive on the
//! thread that installed it (the hook thread) while that thread reads its
//! messages, so what it calls lives with that thread.

use crate::foreground::app_of_window;
use crate::uipi::integrity_level;
use mouser_platform::ForegroundApp;
use std::cell::RefCell;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{EVENT_SYSTEM_FOREGROUND, WINEVENT_OUTOFCONTEXT};

/// What is told the application in front, on each change.
pub(crate) type OnChange = Box<dyn Fn(Option<ForegroundApp>)>;

thread_local! {
    static ON_CHANGE: RefCell<Option<OnChange>> = const { RefCell::new(None) };
}

/// The installed hook. It is removed when dropped, which has to happen on
/// the thread that installed it.
pub(crate) struct ForegroundHook(HWINEVENTHOOK);

impl ForegroundHook {
    /// Installs the hook on this thread: `on_change` is told the application
    /// in front each time the foreground changes. `None` when the system
    /// refuses the hook.
    pub(crate) fn install(on_change: OnChange) -> Option<ForegroundHook> {
        ON_CHANGE.set(Some(on_change));
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                None,
                Some(on_event),
                0,
                0,
                WINEVENT_OUTOFCONTEXT,
            )
        };
        (!hook.is_invalid()).then_some(ForegroundHook(hook))
    }
}

impl Drop for ForegroundHook {
    fn drop(&mut self) {
        let _ = unsafe { UnhookWinEvent(self.0) };
        ON_CHANGE.set(None);
    }
}

/// The hook's callback: the foreground is now `window`.
pub(crate) unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    _event: u32,
    window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    let own = integrity_level(unsafe { GetCurrentProcessId() });
    let app = app_of_window(window, own);
    ON_CHANGE.with_borrow(|on_change| {
        if let Some(on_change) = on_change {
            on_change(app);
        }
    });
}
