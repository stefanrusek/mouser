//! THROWAWAY plan-phase spike for Mouser (001-mouser-v1). Not production code.
//!
//! Safety rules this probe follows:
//! - Every subcommand self-terminates (watchdog hard-exits after 5 s).
//! - Taps are listen-only, except `active`, which installs a filtering tap on
//!   mouse-move events only and swallows them for at most 1.5 s.
//! - No keystrokes are injected. Only small mouse moves; the cursor is restored.
//! - Key events seen by taps are only counted, never logged by key code.

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::runloop::{kCFRunLoopDefaultMode, CFRunLoop};
use core_foundation::string::CFString;
use core_graphics::display::{CGDisplay, CGPoint, CGRect};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement, CGEventType,
    CGMouseButton, CallbackResult, EventField,
};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly,
};
use std::os::raw::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGPreflightPostEventAccess() -> bool;
    fn CGPreflightScreenCaptureAccess() -> bool;
    fn CGGetEventTapList(max: u32, list: *mut TapInfo, count: *mut u32) -> i32;
    fn CGSessionCopyCurrentDictionary() -> CFDictionaryRef;
    fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect) -> bool;
}
#[link(name = "Carbon", kind = "framework")]
extern "C" {
    fn IsSecureEventInputEnabled() -> bool;
    fn EnableSecureEventInput() -> i32;
    fn DisableSecureEventInput() -> i32;
}
#[link(name = "IOKit", kind = "framework")]
extern "C" {
    // IOHIDAccessType IOHIDCheckAccess(IOHIDRequestType) ; 0=Granted 1=Denied 2=Unknown
    fn IOHIDCheckAccess(request: u32) -> u32;
}
extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const i8) -> *mut c_void;
}
const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
struct TapInfo {
    event_tap_id: u32,
    tap_point: u32,
    options: u32,
    events_of_interest: u64,
    tapping_process: i32,
    process_being_tapped: i32,
    enabled: bool,
    min_usec_latency: f32,
    avg_usec_latency: f32,
    max_usec_latency: f32,
}

/// List every event tap currently installed system-wide (who taps what, how).
fn taps() {
    let mut buf = vec![TapInfo::default(); 128];
    let mut n = 0u32;
    let err = unsafe { CGGetEventTapList(128, buf.as_mut_ptr(), &mut n) };
    println!("CGGetEventTapList err={err} count={n}");
    for t in &buf[..n as usize] {
        let o = std::process::Command::new("ps").args(["-p", &t.tapping_process.to_string(), "-o", "comm="]).output().unwrap();
        let name = String::from_utf8_lossy(&o.stdout).trim().rsplit('/').next().unwrap_or("").to_string();
        println!(
            "  tap id={} point={} ({}) options={} ({}) mask={:#x} tapper={} [{}] tapped_pid={} enabled={} avg_latency_us={:.0}",
            t.event_tap_id, t.tap_point,
            ["HID", "Session", "AnnotatedSession"].get(t.tap_point as usize).unwrap_or(&"?"),
            t.options, if t.options == 1 { "ListenOnly" } else { "Default/filter" },
            t.events_of_interest, t.tapping_process, name, t.process_being_tapped, t.enabled, t.avg_usec_latency
        );
    }
}

/// Listen-only for up to 4 s and report source fields of events NOT generated
/// by this process (i.e. real hardware or other injectors), if the user moves.
fn listen_fields() {
    watchdog(5);
    let seen: Arc<std::sync::Mutex<std::collections::BTreeMap<String, u64>>> = Default::default();
    let seen2 = seen.clone();
    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::TailAppendEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::MouseMoved, CGEventType::KeyDown],
        move |_p, ty, e| {
            let k = format!(
                "type={:?} srcPID={} srcStateID={} userData={}",
                ty as u32,
                e.get_integer_value_field(EventField::EVENT_SOURCE_UNIX_PROCESS_ID),
                e.get_integer_value_field(EventField::EVENT_SOURCE_STATE_ID),
                e.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA)
            );
            *seen2.lock().unwrap().entry(k).or_default() += 1;
            CallbackResult::Keep
        },
    )
    .expect("tap create");
    let src = tap.mach_port().create_runloop_source(0).unwrap();
    CFRunLoop::get_current().add_source(&src, unsafe { kCFRunLoopDefaultMode });
    tap.enable();
    run_loop_for(4.0);
    drop(tap);
    let m = seen.lock().unwrap();
    println!("distinct source signatures in 4s: {}", m.len());
    for (k, v) in m.iter() {
        println!("  {k} x{v}");
    }
}

fn watchdog(secs: u64) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        eprintln!("WATCHDOG: hard exit after {secs}s");
        std::process::exit(3);
    });
}

fn access_str(v: u32) -> &'static str {
    match v {
        0 => "Granted",
        1 => "Denied",
        2 => "Unknown",
        _ => "?",
    }
}

fn perms() {
    unsafe {
        println!("AXIsProcessTrusted            = {}", AXIsProcessTrusted());
        println!("CGPreflightListenEventAccess  = {}", CGPreflightListenEventAccess());
        println!("CGPreflightPostEventAccess    = {}", CGPreflightPostEventAccess());
        println!("IOHIDCheckAccess(ListenEvent) = {}", access_str(IOHIDCheckAccess(1)));
        println!("IOHIDCheckAccess(PostEvent)   = {}", access_str(IOHIDCheckAccess(0)));
        println!("CGPreflightScreenCaptureAccess= {}", CGPreflightScreenCaptureAccess());
    }
}

fn cursor_pos() -> CGPoint {
    let src = CGEventSource::new(CGEventSourceStateID::CombinedSessionState).unwrap();
    CGEvent::new(src).unwrap().location()
}

fn run_loop_for(secs: f64) {
    let _ = CFRunLoop::run_in_mode(
        unsafe { kCFRunLoopDefaultMode },
        Duration::from_secs_f64(secs),
        false,
    );
}

/// Listen-only taps. Tries mouse-only and key-only separately so we learn
/// which event classes are gated by which permission.
fn listen(secs: f64) {
    watchdog(5);
    let mouse_types = vec![
        CGEventType::MouseMoved,
        CGEventType::LeftMouseDragged,
        CGEventType::ScrollWheel,
    ];
    let key_types = vec![CGEventType::KeyDown, CGEventType::KeyUp, CGEventType::FlagsChanged];
    for (label, types, loc) in [
        ("mouse@Session", mouse_types.clone(), CGEventTapLocation::Session),
        ("mouse@HID", mouse_types.clone(), CGEventTapLocation::HID),
        ("keys@Session", key_types.clone(), CGEventTapLocation::Session),
        ("keys@HID", key_types.clone(), CGEventTapLocation::HID),
    ] {
        let n = Arc::new(AtomicU64::new(0));
        let n2 = n.clone();
        let t0 = Instant::now();
        let tap = CGEventTap::new(
            loc,
            CGEventTapPlacement::TailAppendEventTap,
            CGEventTapOptions::ListenOnly,
            types,
            move |_p, ty, _e| {
                if !matches!(ty, CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput) {
                    n2.fetch_add(1, Ordering::Relaxed);
                }
                CallbackResult::Keep
            },
        );
        match tap {
            Err(()) => println!("listen tap {label}: CGEventTapCreate returned NULL (create took {:?})", t0.elapsed()),
            Ok(tap) => {
                let src = tap.mach_port().create_runloop_source(0).unwrap();
                CFRunLoop::get_current().add_source(&src, unsafe { kCFRunLoopDefaultMode });
                tap.enable();
                run_loop_for(secs);
                CFRunLoop::get_current().remove_source(&src, unsafe { kCFRunLoopDefaultMode });
                println!(
                    "listen tap {label}: created OK; events received in {secs}s = {}",
                    n.load(Ordering::Relaxed)
                );
            }
        }
    }
}

/// Active (filtering) tap on mouse moves only; swallows moves for <=1.5 s.
fn active() {
    watchdog(4);
    let n = Arc::new(AtomicU64::new(0));
    let n2 = n.clone();
    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::HeadInsertEventTap,
        CGEventTapOptions::Default,
        vec![CGEventType::MouseMoved],
        move |_p, ty, _e| {
            if matches!(ty, CGEventType::MouseMoved) {
                n2.fetch_add(1, Ordering::Relaxed);
                return CallbackResult::Drop;
            }
            CallbackResult::Keep
        },
    );
    match tap {
        Err(()) => println!("active tap (HID, Default, MouseMoved): CGEventTapCreate returned NULL"),
        Ok(tap) => {
            let before = cursor_pos();
            let src = tap.mach_port().create_runloop_source(0).unwrap();
            CFRunLoop::get_current().add_source(&src, unsafe { kCFRunLoopDefaultMode });
            tap.enable();
            // Inject a synthetic move while the filter is active: does it get swallowed?
            let ev_src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
            let target = CGPoint::new(before.x + 7.0, before.y + 7.0);
            let ev = CGEvent::new_mouse_event(ev_src, CGEventType::MouseMoved, target, CGMouseButton::Left).unwrap();
            ev.post(CGEventTapLocation::HID);
            run_loop_for(1.5);
            let after = cursor_pos();
            drop(tap);
            CGDisplay::warp_mouse_cursor_position(before).ok();
            println!(
                "active tap: created OK; MouseMoved swallowed in 1.5s = {}; cursor before={:?} after_injected_move={:?} (restored)",
                n.load(Ordering::Relaxed),
                (before.x, before.y),
                (after.x, after.y)
            );
        }
    }
}

/// Edge-confinement primitives: warp, disassociate, hide; then restore.
fn confine() {
    watchdog(5);
    let orig = cursor_pos();
    println!("orig cursor = ({}, {})", orig.x, orig.y);
    let main = CGDisplay::main();
    let b = main.bounds();
    let center = CGPoint::new(b.origin.x + b.size.width / 2.0, b.origin.y + b.size.height / 2.0);

    let r = CGDisplay::warp_mouse_cursor_position(center);
    let p = cursor_pos();
    println!("CGWarpMouseCursorPosition(center) -> {:?}; cursor now ({}, {})", r, p.x, p.y);

    let r = CGDisplay::associate_mouse_and_mouse_cursor_position(false);
    println!("CGAssociateMouseAndMouseCursorPosition(false) -> {:?}", r);

    // While disassociated, a listen tap should still deliver mouse deltas.
    let dx = Arc::new(AtomicU64::new(0));
    let cnt = Arc::new(AtomicU64::new(0));
    let (dx2, cnt2) = (dx.clone(), cnt.clone());
    let tap = CGEventTap::new(
        CGEventTapLocation::HID,
        CGEventTapPlacement::TailAppendEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::MouseMoved],
        move |_p, _t, e| {
            cnt2.fetch_add(1, Ordering::Relaxed);
            let d = e.get_integer_value_field(EventField::MOUSE_EVENT_DELTA_X).unsigned_abs();
            dx2.fetch_add(d, Ordering::Relaxed);
            CallbackResult::Keep
        },
    );
    let hid = main.hide_cursor();
    println!("CGDisplayHideCursor -> {:?}", hid);
    if let Ok(tap) = &tap {
        let src = tap.mach_port().create_runloop_source(0).unwrap();
        CFRunLoop::get_current().add_source(&src, unsafe { kCFRunLoopDefaultMode });
        tap.enable();
    }
    // Inject a synthetic move (absolute target + delta fields) while disassociated.
    let ev_src = CGEventSource::new(CGEventSourceStateID::HIDSystemState).unwrap();
    let tgt = CGPoint::new(center.x + 20.0, center.y);
    let ev = CGEvent::new_mouse_event(ev_src, CGEventType::MouseMoved, tgt, CGMouseButton::Left).unwrap();
    ev.set_integer_value_field(EventField::MOUSE_EVENT_DELTA_X, 20);
    ev.post(CGEventTapLocation::HID);
    run_loop_for(1.0);
    let p2 = cursor_pos();
    println!(
        "while disassociated 1.0s: tap_ok={} moves_seen={} sum|dx|={} cursor=({}, {})",
        tap.is_ok(),
        cnt.load(Ordering::Relaxed),
        dx.load(Ordering::Relaxed),
        p2.x,
        p2.y
    );
    drop(tap);
    println!("CGDisplayShowCursor -> {:?}", main.show_cursor());
    println!(
        "CGAssociateMouseAndMouseCursorPosition(true) -> {:?}",
        CGDisplay::associate_mouse_and_mouse_cursor_position(true)
    );
    let r = CGDisplay::warp_mouse_cursor_position(orig);
    let p3 = cursor_pos();
    println!("restore warp -> {:?}; cursor now ({}, {})", r, p3.x, p3.y);
}

/// Inject harmless mouse moves via CGEventPost with different sources/taps; restore.
fn inject() {
    watchdog(5);
    perms();
    let orig = cursor_pos();
    println!("orig cursor = ({}, {})", orig.x, orig.y);

    // A listen tap to observe how injected events look to other consumers.
    let seen: Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    let seen2 = seen.clone();
    let my_pid = std::process::id() as i64;
    let tap = CGEventTap::new(
        CGEventTapLocation::Session,
        CGEventTapPlacement::TailAppendEventTap,
        CGEventTapOptions::ListenOnly,
        vec![CGEventType::MouseMoved],
        move |_p, _t, e| {
            let pid = e.get_integer_value_field(EventField::EVENT_SOURCE_UNIX_PROCESS_ID);
            let st = e.get_integer_value_field(EventField::EVENT_SOURCE_STATE_ID);
            let ud = e.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA);
            seen2.lock().unwrap().push(format!(
                "srcPID={pid}{} srcStateID={st} userData={ud}",
                if pid == my_pid { "(self)" } else { "" }
            ));
            CallbackResult::Keep
        },
    );
    if let Ok(tap) = &tap {
        let src = tap.mach_port().create_runloop_source(0).unwrap();
        CFRunLoop::get_current().add_source(&src, unsafe { kCFRunLoopDefaultMode });
        tap.enable();
    }
    println!("observer listen tap ok = {}", tap.is_ok());

    for (i, (label, state, loc)) in [
        ("HIDSystemState source -> kCGHIDEventTap", CGEventSourceStateID::HIDSystemState, CGEventTapLocation::HID),
        ("CombinedSessionState source -> kCGSessionEventTap", CGEventSourceStateID::CombinedSessionState, CGEventTapLocation::Session),
        ("Private source -> kCGHIDEventTap", CGEventSourceStateID::Private, CGEventTapLocation::HID),
    ]
    .into_iter()
    .enumerate()
    {
        let off = 10.0 * (i as f64 + 1.0);
        let target = CGPoint::new(orig.x + off, orig.y + off);
        let src = CGEventSource::new(state).unwrap();
        let ev = CGEvent::new_mouse_event(src, CGEventType::MouseMoved, target, CGMouseButton::Left).unwrap();
        ev.set_integer_value_field(EventField::EVENT_SOURCE_USER_DATA, 0x4D4F55534552); // "MOUSER"
        ev.post(loc);
        run_loop_for(0.15);
        let p = cursor_pos();
        let moved = (p.x - target.x).abs() < 0.5 && (p.y - target.y).abs() < 0.5;
        println!("{label}: target=({}, {}) cursor=({}, {}) moved={moved}", target.x, target.y, p.x, p.y);
        CGDisplay::warp_mouse_cursor_position(orig).ok();
        run_loop_for(0.05);
    }
    drop(tap);
    let p = cursor_pos();
    println!("restored cursor = ({}, {})", p.x, p.y);
    for s in seen.lock().unwrap().iter() {
        println!("  observed injected-move fields: {s}");
    }

    // Low-level alternatives: which symbols exist in this OS?
    for sym in [
        "IOHIDPostEvent",
        "IOHIDUserDeviceCreate",
        "IOHIDUserDeviceCreateWithProperties",
        "IOHIDUserDeviceHandleReportWithTimeStamp",
        "CGEventPostToPid",
        "CGEventPostToPSN",
    ] {
        let c = std::ffi::CString::new(sym).unwrap();
        let p = unsafe { dlsym(RTLD_DEFAULT, c.as_ptr()) };
        println!("dlsym({sym}) = {}", if p.is_null() { "NOT FOUND" } else { "present" });
    }
    virtual_hid_probe();
}

/// Try to create (but never feed) a user-space virtual HID mouse.
fn virtual_hid_probe() {
    type CreateFn = unsafe extern "C" fn(*const c_void, CFDictionaryRef, u32) -> *const c_void;
    let c = std::ffi::CString::new("IOHIDUserDeviceCreateWithProperties").unwrap();
    let f = unsafe { dlsym(RTLD_DEFAULT, c.as_ptr()) };
    if f.is_null() {
        println!("virtual HID: IOHIDUserDeviceCreateWithProperties not found");
        return;
    }
    let f: CreateFn = unsafe { std::mem::transmute(f) };
    // Minimal 3-button relative mouse report descriptor.
    let desc: [u8; 50] = [
        0x05, 0x01, 0x09, 0x02, 0xA1, 0x01, 0x09, 0x01, 0xA1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29,
        0x03, 0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05,
        0x81, 0x03, 0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x15, 0x81, 0x25, 0x7F, 0x75, 0x08, 0x95,
        0x02, 0x81, 0x06, 0xC0, 0xC0,
    ];
    let data = core_foundation::data::CFData::from_buffer(&desc);
    let dict = CFDictionary::from_CFType_pairs(&[
        (CFString::new("ReportDescriptor"), data.as_CFType()),
        (CFString::new("Product"), CFString::new("Mouser spike virtual mouse").as_CFType()),
        (CFString::new("VendorID"), CFNumber::from(0x1234i32).as_CFType()),
        (CFString::new("ProductID"), CFNumber::from(0x5678i32).as_CFType()),
    ]);
    let dev = unsafe { f(std::ptr::null(), dict.as_concrete_TypeRef(), 0) };
    if dev.is_null() {
        println!("virtual HID: IOHIDUserDeviceCreateWithProperties returned NULL (device NOT created)");
    } else {
        println!("virtual HID: device created (no reports sent); releasing");
        unsafe { core_foundation::base::CFRelease(dev) };
    }
}

fn secure() {
    watchdog(5);
    println!("IsSecureEventInputEnabled() = {}", unsafe { IsSecureEventInputEnabled() });
    let d = unsafe { CGSessionCopyCurrentDictionary() };
    if d.is_null() {
        println!("CGSessionCopyCurrentDictionary = NULL");
        return;
    }
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(d as _) };
    let (keys, _vals) = dict.get_keys_and_values();
    let names: Vec<String> = keys
        .iter()
        .map(|k| unsafe { CFString::wrap_under_get_rule(*k as _) }.to_string())
        .collect();
    println!("CGSessionCopyCurrentDictionary keys: {:?}", names);
    match dict.find(CFString::new("kCGSSessionSecureInputPID")) {
        Some(v) => {
            let n = v.downcast::<CFNumber>().and_then(|n| n.to_i64());
            println!("kCGSSessionSecureInputPID = {:?}", n);
            if let Some(pid) = n {
                let out = std::process::Command::new("ps").args(["-p", &pid.to_string(), "-o", "comm="]).output();
                if let Ok(o) = out {
                    println!("  owner process = {}", String::from_utf8_lossy(&o.stdout).trim());
                }
            }
        }
        None => println!("kCGSSessionSecureInputPID: key absent"),
    }
}

/// Take Secure Input ourselves for <=1 s so we can see how the owner is reported.
fn secure_self() {
    watchdog(4);
    println!("self pid = {}", std::process::id());
    println!("EnableSecureEventInput() -> {}", unsafe { EnableSecureEventInput() });
    secure();
    let o = std::process::Command::new("sh")
        .args(["-c", "ioreg -l -w 0 | grep -o 'kCGSSessionSecureInputPID\"=[0-9]*' | sort -u"])
        .output()
        .unwrap();
    println!("ioreg: {}", String::from_utf8_lossy(&o.stdout).trim());
    println!("DisableSecureEventInput() -> {}", unsafe { DisableSecureEventInput() });
    println!("after disable: IsSecureEventInputEnabled() = {}", unsafe { IsSecureEventInputEnabled() });
}

fn displays() {
    let ids = CGDisplay::active_displays().unwrap();
    println!("CGGetActiveDisplayList -> {} display(s)", ids.len());
    for id in ids {
        let d = CGDisplay::new(id);
        let b = d.bounds();
        let mm = d.screen_size();
        let mode = d.display_mode();
        let (mw, mh, pw, ph, hz) = mode
            .as_ref()
            .map(|m| (m.width(), m.height(), m.pixel_width(), m.pixel_height(), m.refresh_rate()))
            .unwrap_or_default();
        let scale = if mw > 0 { pw as f64 / mw as f64 } else { 0.0 };
        let ppi = if mm.width > 0.0 { pw as f64 / (mm.width / 25.4) } else { 0.0 };
        let pt_per_mm = if mm.width > 0.0 { b.size.width / mm.width } else { 0.0 };
        println!(
            "id={id} main={} builtin={} vendor={:#x} model={:#x} serial={:#x} rot={}\n  bounds(points, global)=({}, {}) {}x{}\n  CGDisplayPixelsWide/High={}x{}\n  mode width/height(points)={}x{} pixelWidth/Height={}x{} -> backing scale={scale} refresh={hz}Hz\n  CGDisplayScreenSize={:.1}x{:.1} mm -> {ppi:.1} px/in, {pt_per_mm:.3} points/mm",
            d.is_main(), d.is_builtin(), d.vendor_number(), d.model_number(), d.serial_number(), d.rotation(),
            b.origin.x, b.origin.y, b.size.width, b.size.height,
            d.pixels_wide(), d.pixels_high(),
            mw, mh, pw, ph, mm.width, mm.height
        );
    }
}

fn num(d: &CFDictionary<CFString, CFType>, k: &str) -> Option<i64> {
    d.find(CFString::new(k)).and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_i64())
}
fn s(d: &CFDictionary<CFString, CFType>, k: &str) -> Option<String> {
    d.find(CFString::new(k)).and_then(|v| v.downcast::<CFString>()).map(|s| s.to_string())
}

fn fullscreen() {
    let disp: Vec<(u32, CGRect)> = CGDisplay::active_displays()
        .unwrap()
        .into_iter()
        .map(|id| (id, CGDisplay::new(id).bounds()))
        .collect();
    let arr: CFArray = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        kCGNullWindowID,
    )
    .expect("CGWindowListCopyWindowInfo NULL");
    let arr: CFArray<CFDictionary<CFString, CFType>> = unsafe { CFArray::wrap_under_get_rule(arr.as_concrete_TypeRef()) };
    println!("on-screen windows: {}", arr.len());
    let mut shown_layer0 = 0;
    let mut menubars = 0;
    let mut names_visible = 0;
    for w in arr.iter() {
        let layer = num(&w, "kCGWindowLayer").unwrap_or(-1);
        let owner = s(&w, "kCGWindowOwnerName").unwrap_or_default();
        let name = s(&w, "kCGWindowName");
        if name.is_some() {
            names_visible += 1;
        }
        let mut r = CGRect::default();
        if let Some(bd) = w.find(CFString::new("kCGWindowBounds")) {
            unsafe { CGRectMakeWithDictionaryRepresentation(bd.as_CFTypeRef() as _, &mut r) };
        }
        if owner == "Window Server" && name.as_deref() == Some("Menubar") {
            menubars += 1;
        }
        if layer == 24 && shown_layer0 < 3 {
            println!("  layer24 window owner={owner} name={name:?} bounds=({}, {}) {}x{}", r.origin.x, r.origin.y, r.size.width, r.size.height);
        }
        if layer == 0 && shown_layer0 < 5 {
            shown_layer0 += 1;
            let covers: Vec<u32> = disp
                .iter()
                .filter(|(_, b)| {
                    (b.origin.x - r.origin.x).abs() < 1.0
                        && (b.origin.y - r.origin.y).abs() < 1.0
                        && (b.size.width - r.size.width).abs() < 1.0
                        && (b.size.height - r.size.height).abs() < 1.0
                })
                .map(|(id, _)| *id)
                .collect();
            println!(
                "  layer0 #{shown_layer0} pid={:?} owner={owner} name={:?} bounds=({}, {}) {}x{} exactly_covers_display={:?} alpha={:?} onscreen={:?}",
                num(&w, "kCGWindowOwnerPID"), name, r.origin.x, r.origin.y, r.size.width, r.size.height, covers,
                w.find(CFString::new("kCGWindowAlpha")).and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_f64()),
                w.find(CFString::new("kCGWindowIsOnscreen")).and_then(|v| v.downcast::<CFBoolean>()).map(bool::from),
            );
        }
    }
    println!("windows with readable kCGWindowName: {names_visible}; WindowServer 'Menubar' windows on-screen: {menubars}");
}

fn main() {
    let cmd = std::env::args().nth(1).unwrap_or_default();
    match cmd.as_str() {
        "perms" => perms(),
        "listen" => listen(1.0),
        "active" => active(),
        "confine" => confine(),
        "inject" => inject(),
        "secure" => secure(),
        "taps" => taps(),
        "listen-fields" => listen_fields(),
        "secure-self" => secure_self(),
        "displays" => displays(),
        "fullscreen" => fullscreen(),
        _ => eprintln!("usage: spike_macos_input perms|listen|active|confine|inject|secure|displays|fullscreen"),
    }
}
