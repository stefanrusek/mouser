use crate::displays::*;
use mouser_platform::LocalDisplay;
use std::time::Duration;
use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    PostMessageW, SendMessageW, WM_DISPLAYCHANGE, WM_DPICHANGED,
};

/// A 128-byte EDID: basic block 60 x 34 cm, first detailed timing 597 x 336 mm,
/// and a monitor name descriptor "Mouser 27" in the second slot.
fn edid() -> Vec<u8> {
    let mut e = vec![0u8; 128];
    e[..8].copy_from_slice(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0]);
    e[21] = 60;
    e[22] = 34;
    // DTD at 54: image size bytes 12-14 = 66, 67, 68.
    e[54] = 0x01;
    e[66] = (597 & 0xFF) as u8;
    e[67] = (336 & 0xFF) as u8;
    e[68] = (((597 >> 8) as u8) << 4) | (336 >> 8) as u8;
    // Name descriptor at 72.
    e[72..77].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
    let name = b"Mouser 27\n    ";
    e[77..77 + name.len()].copy_from_slice(name);
    e
}

fn monitor(edid: Option<Vec<u8>>, dpi: u32) -> Monitor {
    Monitor {
        path: r"\\?\DISPLAY#ABC1234#5&1&0&UID4353#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}".into(),
        description: "Generic PnP Monitor".into(),
        rect: RECT {
            left: -2560,
            top: 0,
            right: 0,
            bottom: 1440,
        },
        dpi,
        edid,
    }
}

#[test]
fn edid_size_prefers_the_detailed_timing_then_centimetres() {
    let mut e = edid();
    assert_eq!(edid_size_mm(&e), Some((597.0, 336.0)));
    e[66..69].fill(0);
    assert_eq!(edid_size_mm(&e), Some((600.0, 340.0)));
    e[21] = 0;
    e[22] = 0;
    assert_eq!(edid_size_mm(&e), None, "a projector states no size");
}

#[test]
fn edid_name_comes_from_the_name_descriptor() {
    let mut e = edid();
    assert_eq!(edid_name(&e).as_deref(), Some("Mouser 27"));
    e[75] = 0xFE;
    assert_eq!(edid_name(&e), None);
}

#[test]
fn a_monitor_becomes_a_local_display_with_logical_size_and_physical_origin() {
    let d = to_display(&monitor(Some(edid()), 144));
    assert_eq!(
        d.key,
        r"\\?\DISPLAY#ABC1234#5&1&0&UID4353#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}"
    );
    assert_eq!(d.name, "Mouser 27");
    assert_eq!(d.pixel_size, (2560, 1440));
    assert_eq!(d.scale, 1.5);
    assert_eq!(d.logical_size, (2560.0 / 1.5, 960.0));
    assert_eq!(d.physical_size_mm, Some((597.0, 336.0)));
    assert_eq!(d.origin, (-2560.0, 0.0));
}

#[test]
fn without_an_edid_the_driver_description_names_it_and_size_is_unknown() {
    let d = to_display(&monitor(None, 96));
    assert_eq!(d.name, "Generic PnP Monitor");
    assert_eq!(d.scale, 1.0);
    assert_eq!(d.logical_size, (2560.0, 1440.0));
    assert_eq!(d.physical_size_mm, None);
}

#[test]
fn interface_path_maps_to_the_device_parameters_key() {
    assert_eq!(
        device_parameters_key(
            r"\\?\DISPLAY#PRL5000#3&21de92e8&0&UID0#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}"
        ),
        r"SYSTEM\CurrentControlSet\Enum\DISPLAY\PRL5000\3&21de92e8&0&UID0\Device Parameters"
    );
}

#[test]
fn this_machines_displays_report_scale_and_edid_size() {
    let all = displays().unwrap();
    assert!(!all.is_empty());
    for d in &all {
        assert!(d.scale >= 1.0, "{d:?}");
        assert!(d.key.starts_with(r"\\?\DISPLAY#"), "{d:?}");
        assert!((d.logical_size.0 * f64::from(d.scale) - f64::from(d.pixel_size.0)).abs() < 1.0);
    }
    // The size is known exactly when the monitor's EDID states one (contract
    // rule 4: "where the OS knows it"); a virtual display may have none.
    for d in &all {
        let edid = read_edid(&d.key);
        assert_eq!(
            d.physical_size_mm,
            edid.as_deref().and_then(edid_size_mm),
            "{d:?}"
        );
        if let Some(name) = edid.as_deref().and_then(edid_name) {
            assert_eq!(d.name, name, "{d:?}");
        }
    }
    assert!(
        all.iter().any(|d| d.origin == (0.0, 0.0)),
        "primary at the origin: {all:?}"
    );
}

#[test]
fn display_and_dpi_changes_fire_the_watch() {
    let (tx, rx) = std::sync::mpsc::channel();
    let (hwnd, handle) = start_watch(tx);
    unsafe {
        PostMessageW(
            Some(hwnd),
            WM_DISPLAYCHANGE,
            WPARAM(32),
            LPARAM(1080 << 16 | 1920),
        )
    }
    .unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        displays().unwrap()
    );
    // WM_DPICHANGED carries the suggested window rectangle, so Windows sends it.
    let suggested = RECT {
        left: 0,
        top: 0,
        right: 10,
        bottom: 10,
    };
    unsafe {
        SendMessageW(
            hwnd,
            WM_DPICHANGED,
            Some(WPARAM(144 << 16 | 144)),
            Some(LPARAM(&raw const suggested as isize)),
        )
    };
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)).unwrap(),
        displays().unwrap()
    );
    drop(handle);
    assert!(
        rx.recv_timeout(Duration::from_millis(200)).is_err(),
        "watch ended"
    );
}

#[test]
fn a_watch_whose_receiver_is_gone_keeps_running() {
    let (tx, rx) = std::sync::mpsc::channel();
    drop(rx);
    let (hwnd, handle) = start_watch(tx);
    unsafe { PostMessageW(Some(hwnd), WM_DISPLAYCHANGE, WPARAM(0), LPARAM(0)) }.unwrap();
    std::thread::sleep(Duration::from_millis(200));
    drop(handle);
    let (tx, _rx) = std::sync::mpsc::channel();
    drop(watch(tx).unwrap());
}

fn two_displays() -> Vec<LocalDisplay> {
    let at = |origin: f64, scale: f32, px: u32| LocalDisplay {
        key: String::new(),
        name: String::new(),
        pixel_size: (px, 1000),
        logical_size: (f64::from(px) / f64::from(scale), 1000.0 / f64::from(scale)),
        scale,
        physical_size_mm: None,
        origin: (origin, 0.0),
    };
    vec![at(0.0, 1.0, 2000), at(2000.0, 2.0, 2000)]
}

#[test]
fn pixels_and_logical_points_convert_per_display_scale() {
    let all = two_displays();
    assert_eq!(pixel_to_logical(&all, 500, 40), ((500.0, 40.0), 1.0));
    // 400 px into the 2x display is 200 logical points.
    assert_eq!(pixel_to_logical(&all, 2400, 100), ((2200.0, 50.0), 2.0));
    assert_eq!(logical_to_pixel(&all, 500.0, 40.0), (500, 40));
    assert_eq!(logical_to_pixel(&all, 2200.0, 50.0), (2400, 100));
    // Outside every display: the first display stands in.
    assert_eq!(pixel_to_logical(&all, -5, -5), ((-5.0, -5.0), 1.0));
    assert_eq!(logical_to_pixel(&all, -5.0, 3.0), (-5, 3));
}

#[test]
fn a_point_is_on_a_display_only_inside_one() {
    let all = two_displays();
    assert!(on_a_display(&all, 0.0, 0.0));
    assert!(on_a_display(&all, 2200.0, 50.0));
    assert!(!on_a_display(&all, -5.0, 3.0));
    assert!(!on_a_display(&all, 1_000_000.0, 1_000_000.0));
}

#[test]
fn the_cache_follows_display_changes() {
    let cache = DisplayCache::new();
    assert_eq!(cache.snapshot(), displays().unwrap());
    let before = cache.updates();
    unsafe { PostMessageW(Some(cache.hwnd()), WM_DISPLAYCHANGE, WPARAM(0), LPARAM(0)) }.unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(5);
    while cache.updates() == before {
        assert!(std::time::Instant::now() < end, "no update");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(cache.snapshot(), displays().unwrap());
}
