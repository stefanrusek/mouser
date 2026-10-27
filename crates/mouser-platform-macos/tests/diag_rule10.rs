use std::time::{Duration, Instant};
use mouser_platform::Platform;
use mouser_platform_macos::MacPlatform;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGCursorIsVisible() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayHideCursor(d: u32) -> i32;
    fn CGDisplayShowCursor(d: u32) -> i32;
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(c: i32, t: i32, k: *const std::ffi::c_void, v: *const std::ffi::c_void) -> i32;
}

fn vis() -> bool { unsafe { CGCursorIsVisible() } }

fn poll(label: &str) {
    let t = Instant::now();
    let mut last = vis();
    println!("{label}: t=0 visible={last}");
    while t.elapsed() < Duration::from_millis(1500) {
        let v = vis();
        if v != last { println!("{label}: t={:?} visible={v}", t.elapsed()); last = v; }
        std::thread::sleep(Duration::from_millis(1));
    }
    println!("{label}: end visible={last}");
}

#[test]
fn a_raw() {
    poll("start");
    unsafe { println!("hide(no bg) -> {}", CGDisplayHideCursor(CGMainDisplayID())); }
    poll("after raw hide no-bg");
    unsafe { println!("show -> {}", CGDisplayShowCursor(CGMainDisplayID())); }
    poll("after raw show");
    use core_foundation::base::TCFType;
    let key = core_foundation::string::CFString::from_static_string("SetsCursorInBackground");
    let val = core_foundation::boolean::CFBoolean::true_value();
    unsafe {
        let c = _CGSDefaultConnection();
        println!("setprop -> {}", CGSSetConnectionProperty(c, c, key.as_concrete_TypeRef() as _, val.as_CFTypeRef() as _));
    }
    poll("after setprop");
    unsafe { println!("hide -> {}", CGDisplayHideCursor(CGMainDisplayID())); }
    poll("after raw hide bg");
    unsafe { println!("show -> {}", CGDisplayShowCursor(CGMainDisplayID())); }
    poll("after raw show bg");
}

#[test]
fn b_capture() {
    let p = MacPlatform::new();
    for round in 0..3 {
        let mut c = p.capture().unwrap();
        let (tx, _rx) = std::sync::mpsc::channel();
        c.start(tx).unwrap();
        poll(&format!("r{round} started"));
        c.set_grabbed(true).unwrap();
        poll(&format!("r{round} grabbed"));
        if round == 0 { c.stop(); poll(&format!("r{round} stopped")); }
        drop(c);
        poll(&format!("r{round} dropped"));
    }
}
