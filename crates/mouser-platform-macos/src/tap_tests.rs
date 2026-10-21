use crate::tap::{Context, Tap, Verdict, create_system, trampoline};
use core_foundation::mach_port::CFMachPortRef;
use core_graphics::event::CGEvent;
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use foreign_types::ForeignType;
use mouser_platform::Error;
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn event() -> CGEvent {
    let source = CGEventSource::new(CGEventSourceStateID::Private).unwrap();
    CGEvent::new_keyboard_event(source, 0, true).unwrap()
}

fn context(verdict: Verdict, reenabled: Arc<AtomicUsize>) -> Context {
    Context {
        handler: Box::new(move |_, _| verdict),
        reenable: Box::new(move || {
            reenabled.fetch_add(1, Ordering::SeqCst);
        }),
    }
}

fn call(ctx: &Context, ty: u32, ev: &CGEvent) -> *mut c_void {
    unsafe {
        trampoline(
            std::ptr::null(),
            ty,
            ev.as_ptr().cast(),
            ctx as *const Context as *mut c_void,
        )
    }
}

#[test]
fn handler_verdict_decides_whether_the_event_survives() {
    let ev = event();
    let counter = Arc::new(AtomicUsize::new(0));
    let keep = context(Verdict::Keep, counter.clone());
    assert_eq!(call(&keep, 10, &ev), ev.as_ptr().cast::<c_void>());
    let drop = context(Verdict::Drop, counter.clone());
    assert!(call(&drop, 10, &ev).is_null());
    assert_eq!(counter.load(Ordering::SeqCst), 0);
}

#[test]
fn tap_disabled_notifications_reenable_the_tap_and_skip_the_handler() {
    let ev = event();
    let counter = Arc::new(AtomicUsize::new(0));
    // A dropping handler proves the notification never reaches it.
    let ctx = context(Verdict::Drop, counter.clone());
    assert_eq!(call(&ctx, 0xFFFF_FFFE, &ev), ev.as_ptr().cast::<c_void>());
    assert_eq!(call(&ctx, 0xFFFF_FFFF, &ev), ev.as_ptr().cast::<c_void>());
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[test]
fn refused_tap_creation_is_an_error_naming_the_permissions() {
    fn refuse(_: crate::tap::TapCallback, _: *mut c_void, _: u64) -> CFMachPortRef {
        std::ptr::null_mut()
    }
    let err = Tap::start(refuse, 0, Box::new(|_, _| Verdict::Keep))
        .err()
        .unwrap();
    match err {
        Error::Os(text) => assert!(text.contains("Input Monitoring")),
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn a_real_tap_starts_and_stops() {
    // Needs Accessibility / Input Monitoring: fails loudly without them.
    let tap = Tap::start(create_system, 1 << 5, Box::new(|_, _| Verdict::Keep)).unwrap();
    drop(tap);
}

static CAPTURED_CONTEXT: AtomicUsize = AtomicUsize::new(0);

/// Creates the real tap but remembers the context pointer so the test can
/// deliver a "tap disabled" notification to it.
fn create_and_remember(
    callback: crate::tap::TapCallback,
    info: *mut c_void,
    mask: u64,
) -> CFMachPortRef {
    CAPTURED_CONTEXT.store(info as usize, Ordering::SeqCst);
    create_system(callback, info, mask)
}

#[test]
fn a_real_tap_reenables_itself_when_the_os_disables_it() {
    let tap = Tap::start(create_and_remember, 1 << 5, Box::new(|_, _| Verdict::Drop)).unwrap();
    let ev = event();
    let ctx = CAPTURED_CONTEXT.load(Ordering::SeqCst) as *mut c_void;
    let returned = unsafe { trampoline(std::ptr::null(), 0xFFFF_FFFE, ev.as_ptr().cast(), ctx) };
    assert_eq!(returned, ev.as_ptr().cast::<c_void>());
    drop(tap);
}
