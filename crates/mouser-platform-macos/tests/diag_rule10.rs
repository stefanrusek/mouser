use std::ffi::c_void;
use std::time::Instant;
use core_foundation::base::TCFType;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGCursorIsVisible() -> bool;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayHideCursor(d: u32) -> i32;
    fn CGDisplayShowCursor(d: u32) -> i32;
    fn _CGSDefaultConnection() -> i32;
    fn CGSSetConnectionProperty(c: i32, t: i32, k: *const c_void, v: *const c_void) -> i32;
    fn CGSGetConnectionProperty(c: i32, t: i32, k: *const c_void, v: *mut *const c_void) -> i32;
    fn CGSGetCursorScale(c: i32, s: *mut f32) -> i32;
    fn CGSCurrentCursorSeed() -> i32;
    fn CGWindowListCopyWindowInfo(o: u32, w: u32) -> *const c_void;
}

fn vis() -> bool { unsafe { CGCursorIsVisible() } }

fn key() -> core_foundation::string::CFString {
    core_foundation::string::CFString::from_static_string("SetsCursorInBackground")
}

fn run(name: &str, f: &dyn Fn()) {
    let mut bad = 0;
    let mut hidden_bad = 0;
    let t = Instant::now();
    for _ in 0..50 {
        unsafe { CGDisplayHideCursor(CGMainDisplayID()); }
        if vis() { hidden_bad += 1; }
        unsafe { CGDisplayShowCursor(CGMainDisplayID()); }
        f();
        if !vis() { bad += 1; }
        // let it settle before the next round
        while !vis() { std::thread::yield_now(); }
    }
    println!("{name}: hidden-but-visible {hidden_bad}/50, shown-but-hidden {bad}/50, {:?}", t.elapsed());
}

#[test]
fn flush_candidates() {
    let val = core_foundation::boolean::CFBoolean::true_value();
    unsafe {
        let c = _CGSDefaultConnection();
        assert_eq!(CGSSetConnectionProperty(c, c, key().as_concrete_TypeRef() as _, val.as_CFTypeRef() as _), 0);
    }
    println!("start visible={}", vis());
    run("none", &|| {});
    run("getprop", &|| unsafe {
        let c = _CGSDefaultConnection();
        let mut out: *const c_void = std::ptr::null();
        CGSGetConnectionProperty(c, c, key().as_concrete_TypeRef() as _, &mut out);
    });
    run("cursorscale", &|| unsafe {
        let mut s = 0f32;
        CGSGetCursorScale(_CGSDefaultConnection(), &mut s);
    });
    run("seed", &|| unsafe { CGSCurrentCursorSeed(); });
    run("windowlist", &|| unsafe { CGWindowListCopyWindowInfo(1, 0); });
    run("setprop", &|| unsafe {
        let c = _CGSDefaultConnection();
        let val = core_foundation::boolean::CFBoolean::true_value();
        CGSSetConnectionProperty(c, c, key().as_concrete_TypeRef() as _, val.as_CFTypeRef() as _);
    });
    run("none-again", &|| {});
}
