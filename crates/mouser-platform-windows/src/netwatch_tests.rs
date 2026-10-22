use crate::netwatch::*;
use std::ffi::c_void;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::NetworkManagement::IpHelper::MibParameterNotification;
use windows::Win32::UI::WindowsAndMessaging::{
    PBT_APMRESUMEAUTOMATIC, PBT_APMSUSPEND, SendMessageW, WM_POWERBROADCAST,
};

const WAIT: Duration = Duration::from_secs(5);
const QUIET: Duration = Duration::from_millis(300);

fn power(hwnd: windows::Win32::Foundation::HWND, event: u32) {
    unsafe {
        SendMessageW(
            hwnd,
            WM_POWERBROADCAST,
            Some(WPARAM(event as usize)),
            Some(LPARAM(0)),
        )
    };
}

#[test]
fn resume_from_sleep_fires_the_watch() {
    let (tx, rx) = channel();
    let (hwnd, _watch) = start_watch(tx);
    power(hwnd, PBT_APMRESUMEAUTOMATIC);
    rx.recv_timeout(WAIT).expect("resume fired");
}

#[test]
fn going_to_sleep_and_other_messages_do_not_fire() {
    let (tx, rx) = channel();
    let (hwnd, _watch) = start_watch(tx);
    power(hwnd, PBT_APMSUSPEND);
    unsafe { SendMessageW(hwnd, 0x0400, None, None) };
    assert!(rx.recv_timeout(QUIET).is_err());
}

#[test]
fn an_interface_change_callback_fires_the_watch() {
    let (tx, rx) = channel::<()>();
    let tx: Sender<()> = tx;
    unsafe {
        on_interface_change(
            (&raw const tx).cast::<c_void>(),
            std::ptr::null(),
            MibParameterNotification,
        );
    }
    rx.recv_timeout(WAIT).expect("change fired");
}

#[test]
fn a_watch_whose_receiver_is_gone_ignores_events_and_stops_cleanly() {
    let (tx, rx) = channel();
    let (hwnd, watch) = start_watch(tx);
    drop(rx);
    power(hwnd, PBT_APMRESUMEAUTOMATIC);
    drop(watch);
}

#[test]
fn the_public_watch_starts_and_stops() {
    let (tx, _rx) = channel();
    drop(watch(tx).unwrap());
}

#[test]
fn a_watch_registers_with_the_system_and_cancels_on_drop() {
    // Registration asserts success inside start_watch; dropping cancels it and
    // joins the window thread, so repeating it proves nothing leaks or hangs.
    for _ in 0..3 {
        let (tx, _rx) = channel();
        drop(start_watch(tx).1);
    }
}

#[test]
fn the_platform_exposes_the_watch() {
    use mouser_platform::Platform;
    let (tx, rx) = channel();
    let p = crate::WinPlatform::new();
    let watch = p.watch_network(tx).unwrap();
    drop(watch);
    assert!(rx.recv_timeout(Duration::from_millis(1)).is_err());
}
