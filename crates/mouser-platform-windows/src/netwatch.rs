//! `watch_network`: fires on any interface or address change
//! (`NotifyIpInterfaceChange`, which calls back on a thread-pool thread) and
//! when the PC resumes from sleep (`PBT_APMRESUMEAUTOMATIC`, delivered to a
//! hidden top-level window: message-only windows get no broadcasts).

use crate::dpi;
use mouser_platform::{Result, WatchHandle};
use std::ffi::c_void;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::NetworkManagement::IpHelper::{
    CancelMibChangeNotify2, MIB_IPINTERFACE_ROW, MIB_NOTIFICATION_TYPE, NotifyIpInterfaceChange,
};
use windows::Win32::Networking::WinSock::AF_UNSPEC;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, MSG, PBT_APMRESUMEAUTOMATIC, PostMessageW, PostQuitMessage, RegisterClassW,
    SetWindowLongPtrW, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY, WM_POWERBROADCAST, WNDCLASSW,
    WS_OVERLAPPED,
};
use windows::core::w;

fn fire(tx: &Sender<()>) {
    // A receiver that has gone away just means nobody is listening.
    let _ = tx.send(());
}

/// Runs on a thread-pool thread; `context` is an `Arc<Sender<()>>` the watch
/// keeps alive until the registration is cancelled.
pub(crate) unsafe extern "system" fn on_interface_change(
    context: *const c_void,
    _row: *const MIB_IPINTERFACE_ROW,
    _kind: MIB_NOTIFICATION_TYPE,
) {
    fire(unsafe { &*context.cast::<Sender<()>>() });
}

pub(crate) unsafe extern "system" fn watch_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    match msg {
        WM_POWERBROADCAST => {
            if wp.0 == PBT_APMRESUMEAUTOMATIC as usize {
                let tx = unsafe { &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Sender<()>) };
                fire(tx);
            }
            // TRUE: the broadcast is handled either way.
            LRESULT(1)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// The watch with its window, so tests can post power messages to it.
pub(crate) fn start_watch(tx: Sender<()>) -> (HWND, WatchHandle) {
    let tx = Arc::new(tx);
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let for_window = tx.clone();
    let thread = std::thread::spawn(move || {
        dpi::physical_thread();
        let hinst = unsafe { GetModuleHandleW(None) }.expect("module handle");
        let class = WNDCLASSW {
            lpfnWndProc: Some(watch_proc),
            hInstance: hinst.into(),
            lpszClassName: w!("MouserNetworkWatch"),
            ..Default::default()
        };
        unsafe { RegisterClassW(&class) };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("MouserNetworkWatch"),
                w!("Mouser network watch"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                Some(hinst.into()),
                None,
            )
        }
        .expect("network watch window");
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, Arc::as_ptr(&for_window) as isize) };
        ready_tx.send(hwnd.0 as isize).expect("watch ready");
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            unsafe { DispatchMessageW(&msg) };
        }
    });
    let hwnd = HWND(ready_rx.recv().expect("network watch thread") as _);
    let mut registration = HANDLE::default();
    let status = unsafe {
        NotifyIpInterfaceChange(
            AF_UNSPEC,
            Some(on_interface_change),
            Some(Arc::as_ptr(&tx).cast()),
            false,
            &mut registration,
        )
    };
    assert!(status.is_ok(), "NotifyIpInterfaceChange: {status:?}");
    let raw = hwnd.0 as isize;
    let registration = registration.0 as isize;
    let handle = WatchHandle::new(move || {
        // Waits for a callback in flight, so none runs once this returns.
        let status = unsafe { CancelMibChangeNotify2(HANDLE(registration as _)) };
        assert!(status.is_ok(), "CancelMibChangeNotify2: {status:?}");
        unsafe { PostMessageW(Some(HWND(raw as _)), WM_CLOSE, WPARAM(0), LPARAM(0)) }
            .expect("stop network watch");
        thread.join().expect("network watch thread");
        drop(tx);
    });
    (hwnd, handle)
}

pub(crate) fn watch(tx: Sender<()>) -> Result<WatchHandle> {
    Ok(start_watch(tx).1)
}
