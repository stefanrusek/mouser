//! `watch_network`: fires on any interface address change (an
//! `SCDynamicStore` notification on `State:/Network/Interface/*/IPv4|IPv6`)
//! and when the Mac wakes from sleep (IOKit system power notification).

use core_foundation_sc::array::CFArray;
use core_foundation_sc::base::TCFType;
use core_foundation_sc::runloop::{
    CFRunLoop, CFRunLoopSourceRef, kCFRunLoopCommonModes, kCFRunLoopDefaultMode,
};
use core_foundation_sc::string::CFString;
use mouser_platform::{Result, WatchHandle};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use system_configuration::dynamic_store::{
    SCDynamicStore, SCDynamicStoreBuilder, SCDynamicStoreCallBackContext, SCDynamicStoreCallBackT,
};

pub(crate) const PATTERNS: [&str; 2] = [
    "State:/Network/Interface/[^/]+/IPv4",
    "State:/Network/Interface/[^/]+/IPv6",
];

const CAN_SYSTEM_SLEEP: u32 = 0xE000_0270;
const SYSTEM_WILL_SLEEP: u32 = 0xE000_0280;
const SYSTEM_HAS_POWERED_ON: u32 = 0xE000_0300;

type PowerCallback = unsafe extern "C" fn(*mut c_void, u32, u32, *mut c_void);

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IORegisterForSystemPower(
        refcon: *mut c_void,
        notify_port: *mut *mut c_void,
        callback: PowerCallback,
        notifier: *mut u32,
    ) -> u32;
    fn IODeregisterForSystemPower(notifier: *mut u32) -> i32;
    fn IOAllowPowerChange(kernel_port: u32, notification_id: isize) -> i32;
    fn IONotificationPortGetRunLoopSource(notify: *mut c_void) -> CFRunLoopSourceRef;
    fn IONotificationPortDestroy(notify: *mut c_void);
    fn IOServiceClose(connect: u32) -> i32;
}

/// State for the power callback.
pub(crate) struct PowerContext {
    pub tx: Mutex<Sender<()>>,
    /// The root power domain connection, needed to acknowledge sleep.
    pub root_port: AtomicU32,
}

/// Wake fires the watch; sleep requests are acknowledged so the Mac is never
/// kept awake by Mouser.
pub(crate) unsafe extern "C" fn power_event(
    refcon: *mut c_void,
    _service: u32,
    message: u32,
    argument: *mut c_void,
) {
    let ctx = unsafe { &*(refcon as *const PowerContext) };
    match message {
        SYSTEM_HAS_POWERED_ON => {
            // The receiver going away just means nobody is listening any more.
            let _ = ctx.tx.lock().unwrap().send(());
        }
        CAN_SYSTEM_SLEEP | SYSTEM_WILL_SLEEP => {
            unsafe { IOAllowPowerChange(ctx.root_port.load(Ordering::SeqCst), argument as isize) };
        }
        _ => {}
    }
}

/// `SCDynamicStore` callback: an interface address changed.
pub(crate) fn address_changed(
    _store: SCDynamicStore,
    _changed: CFArray<CFString>,
    tx: &mut Sender<()>,
) {
    let _ = tx.send(());
}

pub(crate) fn watch(tx: Sender<()>) -> Result<WatchHandle> {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_in_thread = stop.clone();
    let (ready_tx, ready_rx) = channel();
    let thread = std::thread::spawn(move || {
        let store = SCDynamicStoreBuilder::new("com.mouser.netwatch")
            .callback_context(SCDynamicStoreCallBackContext {
                callout: address_changed as SCDynamicStoreCallBackT<Sender<()>>,
                info: tx.clone(),
            })
            .build()
            .expect("SCDynamicStoreCreate");
        let keys = CFArray::<CFString>::from_CFTypes(&[]);
        let patterns: Vec<CFString> = PATTERNS.iter().map(|p| CFString::new(p)).collect();
        assert!(
            store.set_notification_keys(&keys, &CFArray::from_CFTypes(&patterns)),
            "SCDynamicStoreSetNotificationKeys"
        );
        let run_loop = CFRunLoop::get_current();
        let source = store.create_run_loop_source().expect("SC run loop source");
        run_loop.add_source(&source, unsafe { kCFRunLoopCommonModes });

        let power = Box::into_raw(Box::new(PowerContext {
            tx: Mutex::new(tx),
            root_port: AtomicU32::new(0),
        }));
        let mut port: *mut c_void = std::ptr::null_mut();
        let mut notifier = 0u32;
        let root = unsafe {
            IORegisterForSystemPower(power as *mut c_void, &mut port, power_event, &mut notifier)
        };
        assert!(root != 0, "IORegisterForSystemPower");
        unsafe { &*power }.root_port.store(root, Ordering::SeqCst);
        let power_source = unsafe { IONotificationPortGetRunLoopSource(port) };
        let power_source = unsafe {
            core_foundation_sc::runloop::CFRunLoopSource::wrap_under_get_rule(power_source)
        };
        run_loop.add_source(&power_source, unsafe { kCFRunLoopCommonModes });

        ready_tx.send(run_loop).unwrap();
        while !stop_in_thread.load(Ordering::SeqCst) {
            let _ = CFRunLoop::run_in_mode(
                unsafe { kCFRunLoopDefaultMode },
                Duration::from_secs(1),
                false,
            );
        }
        unsafe {
            IODeregisterForSystemPower(&mut notifier);
            IOServiceClose(root);
            IONotificationPortDestroy(port);
            drop(Box::from_raw(power));
        }
    });
    let run_loop = ready_rx.recv().unwrap();
    Ok(WatchHandle::new(move || {
        stop.store(true, Ordering::SeqCst);
        run_loop.stop();
        thread.join().unwrap();
    }))
}
