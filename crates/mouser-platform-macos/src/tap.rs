//! One HID-level filtering `CGEventTap` on a dedicated run-loop thread.
//!
//! The tap is created through raw FFI rather than `core_graphics::CGEventTap`
//! because that wrapper types the callback's event type as an exhaustive Rust
//! enum, which is undefined behaviour for `NX_SYSDEFINED` and the gesture
//! events a grab must swallow.

use core_foundation::base::TCFType;
use core_foundation::mach_port::{CFMachPort, CFMachPortInvalidate, CFMachPortRef};
use core_foundation::runloop::{CFRunLoop, kCFRunLoopCommonModes, kCFRunLoopDefaultMode};
use core_graphics::event::CGEvent;
use foreign_types::ForeignType;
use mouser_platform::{Error, Result};
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::thread::JoinHandle;
use std::time::Duration;

/// `kCGEventTapDisabledByTimeout` / `kCGEventTapDisabledByUserInput`.
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Keep,
    Drop,
}

pub(crate) type Handler = dyn Fn(u32, &CGEvent) -> Verdict + Send + Sync;

/// Everything the C callback needs, reachable through `user_info`.
pub(crate) struct Context {
    pub handler: Box<Handler>,
    /// Turns the tap back on after the OS disabled it.
    pub reenable: Box<dyn Fn() + Send + Sync>,
}

pub(crate) type TapCallback =
    unsafe extern "C" fn(*const c_void, u32, *mut c_void, *mut c_void) -> *mut c_void;

/// Creates the tap: `(callback, user_info, mask)` -> mach port, null on denial.
pub(crate) type Create = fn(TapCallback, *mut c_void, u64) -> CFMachPortRef;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: TapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
}

/// HID event tap point, head insertion, active (filtering) tap.
pub(crate) fn create_system(callback: TapCallback, info: *mut c_void, mask: u64) -> CFMachPortRef {
    unsafe { CGEventTapCreate(0, 0, 0, mask, callback, info) }
}

/// The C callback: out-of-band "tap disabled" notifications re-enable the
/// tap; everything else goes to the handler.
pub(crate) unsafe extern "C" fn trampoline(
    _proxy: *const c_void,
    ty: u32,
    event: *mut c_void,
    info: *mut c_void,
) -> *mut c_void {
    let ctx = unsafe { &*(info as *const Context) };
    if ty >= TAP_DISABLED_BY_TIMEOUT {
        (ctx.reenable)();
        return event;
    }
    let cg_event = ManuallyDrop::new(unsafe { CGEvent::from_ptr(event as *mut _) });
    match (ctx.handler)(ty, &cg_event) {
        Verdict::Keep => event,
        Verdict::Drop => std::ptr::null_mut(),
    }
}

/// A running tap; dropping it stops the thread and removes the tap.
pub(crate) struct Tap {
    run_loop: CFRunLoop,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Tap {
    pub(crate) fn start(create: Create, mask: u64, handler: Box<Handler>) -> Result<Tap> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_in_thread = stop.clone();
        let (ready_tx, ready_rx) = channel();
        let thread = std::thread::spawn(move || {
            // The port address is shared with the re-enable closure; this
            // thread keeps the port alive for as long as the callback can run.
            let port_cell = Arc::new(AtomicUsize::new(0));
            let cell = port_cell.clone();
            let ctx = Box::into_raw(Box::new(Context {
                handler,
                reenable: Box::new(move || unsafe {
                    CGEventTapEnable(cell.load(Ordering::SeqCst) as CFMachPortRef, true)
                }),
            }));
            let port = create(trampoline, ctx as *mut c_void, mask);
            if port.is_null() {
                drop(unsafe { Box::from_raw(ctx) });
                let denied = Error::Os(
                    "CGEventTapCreate was refused: grant Accessibility and Input Monitoring".into(),
                );
                ready_tx.send(Err(denied)).unwrap();
                return;
            }
            port_cell.store(port as usize, Ordering::SeqCst);
            let mach_port = unsafe { CFMachPort::wrap_under_create_rule(port) };
            let source = mach_port.create_runloop_source(0).expect("run loop source");
            let run_loop = CFRunLoop::get_current();
            run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });
            unsafe { CGEventTapEnable(port, true) };
            ready_tx.send(Ok(run_loop)).unwrap();
            while !stop_in_thread.load(Ordering::SeqCst) {
                let _ = CFRunLoop::run_in_mode(
                    unsafe { kCFRunLoopDefaultMode },
                    Duration::from_secs(1),
                    false,
                );
            }
            unsafe {
                CGEventTapEnable(port, false);
                CFMachPortInvalidate(port);
            }
            drop(unsafe { Box::from_raw(ctx) });
        });
        let run_loop = ready_rx.recv().unwrap()?;
        Ok(Tap {
            run_loop,
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.run_loop.stop();
        self.thread.take().unwrap().join().unwrap();
    }
}
