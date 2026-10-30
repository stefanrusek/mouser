//! Following the input desktop (research V9; Story 5 sc. 1–2). The session
//! engine runs as SYSTEM, so it may open the login screen's and the
//! permission prompt's desktop. When another desktop gets the input, the
//! hook thread moves there: hooks bind a thread to its desktop, so it takes
//! them off, switches with `SetThreadDesktop` and puts them back. Injection
//! follows on its next send. The check runs each watchdog cycle; there is
//! no timer of its own. A process that may not open the other desktop (a
//! user's own Mouser) stays where it is, and the watchdog takes the capture
//! away as before.

use crate::diag::{self, DiagDesktop, DiagEvent};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, DESKTOP_ACCESS_FLAGS, DESKTOP_CONTROL_FLAGS, DESKTOP_CREATEMENU,
    DESKTOP_CREATEWINDOW, DESKTOP_ENUMERATE, DESKTOP_HOOKCONTROL, DESKTOP_READOBJECTS,
    DESKTOP_SWITCHDESKTOP, DESKTOP_WRITEOBJECTS, HDESK, OpenInputDesktop, SetThreadDesktop,
};

/// Which desktop has the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum InputDesk {
    /// The user's own.
    Default = 0,
    /// The login screen, the lock screen and permission prompts.
    Winlogon = 1,
    /// Any other (a screen saver).
    Other = 2,
}

impl InputDesk {
    pub(crate) fn of_name(name: &str) -> InputDesk {
        if name.eq_ignore_ascii_case("Default") {
            InputDesk::Default
        } else if name.eq_ignore_ascii_case("Winlogon") {
            InputDesk::Winlogon
        } else {
            InputDesk::Other
        }
    }

    pub(crate) fn diag(self) -> DiagDesktop {
        match self {
            InputDesk::Default => DiagDesktop::Default,
            InputDesk::Winlogon => DiagDesktop::Winlogon,
            InputDesk::Other => DiagDesktop::Other,
        }
    }

    fn of_u8(value: u8) -> InputDesk {
        match value {
            1 => InputDesk::Winlogon,
            2 => InputDesk::Other,
            _ => InputDesk::Default,
        }
    }
}

/// What the hook thread tells everyone else: the desktop it followed to,
/// and how many times it switched, so injection knows to follow.
#[derive(Default)]
pub(crate) struct Published {
    followed: AtomicU8,
    generation: AtomicU64,
}

impl Published {
    pub(crate) fn followed(&self) -> InputDesk {
        InputDesk::of_u8(self.followed.load(Ordering::SeqCst))
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// The hook thread is now on `desk`.
    pub(crate) fn switched(&self, desk: InputDesk) {
        self.followed.store(desk as u8, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
}

/// This process's: there is one capture per process.
pub(crate) static THIS_PROCESS: Published = Published {
    followed: AtomicU8::new(0),
    generation: AtomicU64::new(0),
};

/// The system calls desktop following makes.
pub(crate) trait DesktopApi {
    type Handle;
    /// The desktop that has the input, opened so a thread can switch to it,
    /// and its name; `None` when this process may not open it.
    fn open_input(&self) -> Option<(Self::Handle, String)>;
    /// Makes `desk` the calling thread's desktop; whether that worked.
    fn set_thread(&self, desk: &Self::Handle) -> bool;
    fn close(&self, desk: Self::Handle);
}

/// The hooks a thread holds, which bind it to its desktop.
pub(crate) trait ThreadHooks {
    fn remove(&mut self);
    fn install(&mut self);
}

/// The hook thread's view of desktops.
pub(crate) struct Follower<'a, A: DesktopApi> {
    api: A,
    published: &'a Published,
    /// The desktop the thread switched to, kept open while it is used.
    held: Option<A::Handle>,
    name: String,
}

impl<'a, A: DesktopApi> Follower<'a, A> {
    /// A follower for a thread on the user's own desktop.
    pub(crate) fn new(api: A, published: &'a Published) -> Self {
        Follower {
            api,
            published,
            held: None,
            name: "Default".into(),
        }
    }

    /// When another desktop has the input, moves the calling thread and its
    /// `hooks` there, has injection follow and says so. The desktop moved
    /// to, if any.
    pub(crate) fn check(&mut self, hooks: &mut dyn ThreadHooks) -> Option<InputDesk> {
        let (desk, name) = self.api.open_input()?;
        if name == self.name {
            self.api.close(desk);
            return None;
        }
        hooks.remove();
        let switched = self.api.set_thread(&desk);
        hooks.install();
        if !switched {
            self.api.close(desk);
            return None;
        }
        if let Some(left) = self.held.replace(desk) {
            self.api.close(left);
        }
        let to = InputDesk::of_name(&name);
        self.name = name;
        self.published.switched(to);
        diag::note(DiagEvent::CaptureDesktop { desktop: to.diag() });
        Some(to)
    }
}

/// Before an injection: when the hook thread switched desktops since this
/// thread last looked (`seen`), moves this thread to the input desktop too.
/// Whether it tried.
pub(crate) fn follow_for_injection<A: DesktopApi>(
    api: &A,
    published: &Published,
    seen: &mut u64,
) -> bool {
    let now = published.generation();
    if *seen == now {
        return false;
    }
    *seen = now;
    if let Some((desk, _)) = api.open_input() {
        // Kept open: the thread now uses it.
        let _ = api.set_thread(&desk);
    }
    true
}

thread_local! {
    /// The switches this thread has followed.
    static SEEN: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Before this thread injects: follows the hook thread's last switch, once.
pub(crate) fn follow_this_thread() {
    SEEN.with(|seen| {
        let mut now = seen.get();
        follow_for_injection(&System, &THIS_PROCESS, &mut now);
        seen.set(now);
    });
}

/// A desktop handle; a kernel handle may be used from any thread.
pub(crate) struct Desk(HDESK);

unsafe impl Send for Desk {}
unsafe impl Sync for Desk {}

/// The real system calls.
pub(crate) struct System;

impl DesktopApi for System {
    type Handle = Desk;

    fn open_input(&self) -> Option<(Desk, String)> {
        let access = DESKTOP_ACCESS_FLAGS(
            DESKTOP_CREATEMENU.0
                | DESKTOP_CREATEWINDOW.0
                | DESKTOP_ENUMERATE.0
                | DESKTOP_HOOKCONTROL.0
                | DESKTOP_WRITEOBJECTS.0
                | DESKTOP_READOBJECTS.0
                | DESKTOP_SWITCHDESKTOP.0,
        );
        // DF_ALLOWOTHERACCOUNTHOOK: hooks of other accounts may be set on it.
        let flags = DESKTOP_CONTROL_FLAGS(1);
        let desk = unsafe { OpenInputDesktop(flags, false, access) }.ok()?;
        let name = crate::watchdog::desktop_name(desk);
        Some((Desk(desk), name))
    }

    fn set_thread(&self, desk: &Desk) -> bool {
        unsafe { SetThreadDesktop(desk.0) }.is_ok()
    }

    fn close(&self, desk: Desk) {
        let _ = unsafe { CloseDesktop(desk.0) };
    }
}
