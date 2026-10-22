//! Probes 5 and 6 (X11 half) of FINDINGS-windows-linux: X11 grab failures while another client
//! holds a grab (as an open menu does) and XFixes pointer barriers for corners.
//! Prints `PROBE key=value` lines. Run under Xvfb (or a real X11 session).
//!
//! usage: spike_lock_drag_x11 holder SECS            hold XGrabPointer+XGrabKeyboard on the root window
//!        spike_lock_drag_x11 grab [MAX_SECS]        try to grab like Mouser's capture would, retrying with backoff
//!        spike_lock_drag_x11 barrier                XFixes barrier tests (edge, corner L, XWarpPointer bypass)

use std::time::{Duration, Instant};

use x11rb::{
    connection::Connection,
    protocol::{
        xfixes::{BarrierDirections, ConnectionExt as _},
        xproto::{ConnectionExt as _, EventMask, GrabMode, GrabStatus, Window},
        xtest::ConnectionExt as _,
    },
    rust_connection::RustConnection,
};

fn p(key: &str, val: impl std::fmt::Debug) {
    println!("PROBE {key}={val:?}");
}

fn try_grab(c: &RustConnection, root: Window) -> (GrabStatus, GrabStatus) {
    let mask = EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION;
    let ptr = c
        .grab_pointer(false, root, mask, GrabMode::ASYNC, GrabMode::ASYNC, x11rb::NONE, x11rb::NONE, x11rb::CURRENT_TIME)
        .unwrap()
        .reply()
        .unwrap()
        .status;
    let kbd = c
        .grab_keyboard(false, root, x11rb::CURRENT_TIME, GrabMode::ASYNC, GrabMode::ASYNC)
        .unwrap()
        .reply()
        .unwrap()
        .status;
    (ptr, kbd)
}

fn holder(secs: u64) {
    let (c, s) = x11rb::connect(None).unwrap();
    let root = c.setup().roots[s].root;
    p("holder.grab", try_grab(&c, root));
    std::thread::sleep(Duration::from_secs(secs));
    c.ungrab_pointer(x11rb::CURRENT_TIME).unwrap();
    c.ungrab_keyboard(x11rb::CURRENT_TIME).unwrap();
    c.flush().unwrap();
    p("holder.released", true);
}

/// Mouser's capture must grab to swallow local input. If another client holds a grab (an open menu),
/// GrabPointer returns AlreadyGrabbed; retry with backoff until it succeeds or we give up.
fn grab(max_secs: u64) {
    let (c, s) = x11rb::connect(None).unwrap();
    let root = c.setup().roots[s].root;
    let t0 = Instant::now();
    let mut delay = Duration::from_millis(20);
    let mut attempt = 0;
    loop {
        attempt += 1;
        let (ptr, kbd) = try_grab(&c, root);
        p("grab.attempt", (attempt, t0.elapsed().as_millis() as u64, format!("{ptr:?}"), format!("{kbd:?}")));
        if ptr == GrabStatus::SUCCESS && kbd == GrabStatus::SUCCESS {
            p("grab.acquired_after_ms", t0.elapsed().as_millis() as u64);
            return;
        }
        // A half-acquired pair must be released before retrying.
        c.ungrab_pointer(x11rb::CURRENT_TIME).unwrap();
        c.ungrab_keyboard(x11rb::CURRENT_TIME).unwrap();
        c.flush().unwrap();
        if t0.elapsed() > Duration::from_secs(max_secs) {
            p("grab.gave_up_after_ms", t0.elapsed().as_millis() as u64);
            return;
        }
        std::thread::sleep(delay);
        delay = (delay * 2).min(Duration::from_millis(500));
    }
}

fn pointer(c: &RustConnection, root: Window) -> (i16, i16) {
    let r = c.query_pointer(root).unwrap().reply().unwrap();
    (r.root_x, r.root_y)
}

/// Relative motion. With MOTION_CMD=<script> (a uinput helper) it is real kernel-device motion through
/// libinput/Xorg; otherwise XTest (what Xvfb offers).
fn rel_motion(c: &RustConnection, root: Window, dx: i16, dy: i16) {
    if let Ok(cmd) = std::env::var("MOTION_CMD") {
        let st = std::process::Command::new("python3").args([&cmd, "send", &format!("move {dx} {dy}")]).status().unwrap();
        assert!(st.success());
        std::thread::sleep(Duration::from_millis(350));
    } else {
        // XTest MotionNotify with detail=1 means relative.
        c.xtest_fake_input(6, 1, 0, root, dx, dy, 0).unwrap();
        c.flush().unwrap();
        c.get_input_focus().unwrap().reply().unwrap();
    }
}

/// Absolute placement with XWarpPointer.
fn warp_to(c: &RustConnection, root: Window, x: i16, y: i16) {
    c.warp_pointer(x11rb::NONE, root, 0, 0, 0, 0, x, y).unwrap();
    c.flush().unwrap();
    c.get_input_focus().unwrap().reply().unwrap();
}

fn barrier() {
    let (c, s) = x11rb::connect(None).unwrap();
    let screen = &c.setup().roots[s];
    let root = screen.root;
    let (w, h) = (screen.width_in_pixels as i16, screen.height_in_pixels as i16);
    let v = c.xfixes_query_version(5, 0).unwrap().reply().unwrap();
    p("xfixes.version", (v.major_version, v.minor_version));
    p("screen", (w, h));
    // Device list for the barriers: empty = (per the protocol text) all master devices; BARRIER_DEVICES=1,2 overrides.
    let devs: Vec<u16> = std::env::var("BARRIER_DEVICES").map(|v| v.split(',').map(|d| d.parse().unwrap()).collect()).unwrap_or_default();
    p("barrier.devices", &devs);

    // 1. A vertical barrier at x=1000 blocking motion in +x (XFixes `directions` lists the directions that are
    // ALLOWED through, so NEGATIVE_X = block +x): pointer moving right from x=900 by 300.
    let id = c.generate_id().unwrap();
    c.xfixes_create_pointer_barrier(id, root, 1000, 0, 1000, h as u16, BarrierDirections::NEGATIVE_X, &devs).unwrap().check().expect("CreatePointerBarrier");
    c.flush().unwrap();
    warp_to(&c, root, 900, 500);
    rel_motion(&c, root, 300, 0);
    p("barrier.edge.pointer_after_+300_from_900", pointer(&c, root));
    // moving back left is allowed
    rel_motion(&c, root, -100, 0);
    p("barrier.edge.pointer_after_-100", pointer(&c, root));
    // absolute warp (XTest absolute / XWarpPointer) is not blocked by barriers
    warp_to(&c, root, 1200, 500);
    p("barrier.edge.pointer_after_XWarpPointer_to_1200", pointer(&c, root));
    c.xfixes_delete_pointer_barrier(id).unwrap();

    // 2. Corner block: two barriers forming an L at the top-right screen corner region so that
    // a diagonal move cannot slip through the corner between a horizontal and a vertical barrier.
    // Region: x in [w-200, w], y in [0, 200]. Horizontal barrier y=200 blocks -y (upward; allows +y) for x>=w-200;
    // vertical barrier x=w-200 blocks +x for y<=200.
    // CORNER_OVERLAP=N extends each barrier N px past the corner so the two overlap (no diagonal gap).
    let ov: u16 = std::env::var("CORNER_OVERLAP").map(|v| v.parse().unwrap()).unwrap_or(0);
    p("barrier.corner.overlap_px", ov);
    let h_id = c.generate_id().unwrap();
    let v_id = c.generate_id().unwrap();
    c.xfixes_create_pointer_barrier(h_id, root, (w - 200) as u16 - ov, 200, w as u16, 200, BarrierDirections::POSITIVE_Y, &devs).unwrap();
    c.xfixes_create_pointer_barrier(v_id, root, (w - 200) as u16, 0, (w - 200) as u16, 200 + ov, BarrierDirections::NEGATIVE_X, &devs).unwrap();
    c.flush().unwrap();
    // Approach the corner diagonally from inside: (w-300, 300) moving (+250,-250).
    warp_to(&c, root, w - 300, 300);
    for _ in 0..25 {
        rel_motion(&c, root, 10, -10);
    }
    p("barrier.corner.diagonal_into_blocked_corner", pointer(&c, root));
    // Straight up from x = w-100 (inside the blocked column) from y=300.
    warp_to(&c, root, w - 100, 300);
    rel_motion(&c, root, 0, -250);
    p("barrier.corner.straight_up_through_horizontal_barrier", pointer(&c, root));
    // Straight right along y=100 starting at x=w-300.
    warp_to(&c, root, w - 300, 100);
    rel_motion(&c, root, 250, 0);
    p("barrier.corner.straight_right_through_vertical_barrier", pointer(&c, root));
    // Motion outside the blocked region is free: left/down from (w-300, 300).
    warp_to(&c, root, w - 300, 300);
    rel_motion(&c, root, 100, 0);
    p("barrier.corner.free_motion_right_at_y300", pointer(&c, root));
    // And the barrier events (XFixes 5 BarrierHit) are available to the owner:
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("holder") => holder(args[1].parse().unwrap()),
        Some("grab") => grab(args.get(1).map_or(10, |s| s.parse().unwrap())),
        Some("barrier") => barrier(),
        _ => panic!("usage: spike_lock_drag_x11 holder SECS | grab [MAX_SECS] | barrier"),
    }
}
