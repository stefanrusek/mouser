use crate::netwatch::{PATTERNS, PowerContext, address_changed, power_event, watch};
use core_foundation_sc::array::CFArray;
use core_foundation_sc::string::CFString;
use std::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc::channel;
use std::time::Duration;
use system_configuration::dynamic_store::SCDynamicStoreBuilder;

const CAN_SYSTEM_SLEEP: u32 = 0xE000_0270;
const SYSTEM_WILL_SLEEP: u32 = 0xE000_0280;
const SYSTEM_HAS_POWERED_ON: u32 = 0xE000_0300;

fn power_context() -> (PowerContext, std::sync::mpsc::Receiver<()>) {
    let (tx, rx) = channel();
    (
        PowerContext {
            tx: Mutex::new(tx),
            root_port: AtomicU32::new(0),
        },
        rx,
    )
}

fn deliver(ctx: &PowerContext, message: u32) {
    unsafe {
        power_event(
            ctx as *const PowerContext as *mut c_void,
            0,
            message,
            std::ptr::null_mut(),
        )
    };
}

#[test]
fn watched_patterns_are_interface_ipv4_and_ipv6_state() {
    assert_eq!(
        PATTERNS,
        [
            "State:/Network/Interface/[^/]+/IPv4",
            "State:/Network/Interface/[^/]+/IPv6"
        ]
    );
}

#[test]
fn an_address_change_notification_fires_the_watch() {
    let (tx, rx) = channel();
    let mut tx = tx;
    let store = SCDynamicStoreBuilder::new("mouser-test").build().unwrap();
    address_changed(store, CFArray::<CFString>::from_CFTypes(&[]), &mut tx);
    assert_eq!(rx.try_recv(), Ok(()));
}

#[test]
fn waking_from_sleep_fires_the_watch() {
    let (ctx, rx) = power_context();
    deliver(&ctx, SYSTEM_HAS_POWERED_ON);
    assert_eq!(rx.try_recv(), Ok(()));
}

#[test]
fn sleep_requests_are_acknowledged_without_firing() {
    let (ctx, rx) = power_context();
    deliver(&ctx, CAN_SYSTEM_SLEEP);
    deliver(&ctx, SYSTEM_WILL_SLEEP);
    deliver(&ctx, 0x1234);
    assert!(rx.try_recv().is_err());
}

#[test]
fn the_watch_registers_with_the_os_and_stops_when_dropped() {
    let (tx, rx) = channel();
    let handle = watch(tx).unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    drop(handle);
    assert!(rx.recv().is_err());
}
