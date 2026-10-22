//! Probe logger: repeats the capture watchdog's probe every 5 s and appends one
//! line per probe to %TEMP%\probe_logger.log. Standalone; not part of the workspace.
//! Usage: probe_logger [seconds]   (default 360)
use std::io::Write;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::SeqCst};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const TAG: usize = 0x4D4F_5550;
static M: AtomicU32 = AtomicU32::new(0);
static K: AtomicU32 = AtomicU32::new(0);
static M_ANY: AtomicU64 = AtomicU64::new(0);

unsafe extern "system" fn mproc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let i = unsafe { &*(lp.0 as *const MSLLHOOKSTRUCT) };
        if i.dwExtraInfo == TAG {
            M.fetch_add(1, SeqCst);
            return LRESULT(1);
        }
        M_ANY.fetch_add(1, SeqCst);
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}
unsafe extern "system" fn kproc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let i = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if i.dwExtraInfo == TAG {
            K.fetch_add(1, SeqCst);
            return LRESULT(1);
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

fn last_input() -> u32 {
    let mut l = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
    unsafe { let _ = GetLastInputInfo(&mut l); }
    l.dwTime
}

fn fg() -> String {
    unsafe {
        let h = GetForegroundWindow();
        let mut t = [0u16; 80];
        let n = GetWindowTextW(h, &mut t);
        format!("{:?}:{}", h.0, String::from_utf16_lossy(&t[..n as usize]))
    }
}

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(360);
    std::thread::spawn(|| unsafe {
        let h = GetModuleHandleW(None).unwrap();
        let _m = SetWindowsHookExW(WH_MOUSE_LL, Some(mproc), Some(h.into()), 0).unwrap();
        let _k = SetWindowsHookExW(WH_KEYBOARD_LL, Some(kproc), Some(h.into()), 0).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {}
    });
    std::thread::sleep(Duration::from_secs(1));
    let path = format!(r"{}\probe_logger.log", std::env::var("TEMP").unwrap());
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path).unwrap();
    let arch = std::env::consts::ARCH;
    writeln!(f, "# start arch={arch} secs={secs}").unwrap();
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(secs) {
        let (m0, k0) = (M.load(SeqCst), K.load(SeqCst));
        let (li0, tick0) = (last_input(), unsafe { GetTickCount() });
        let mut mi = INPUT { r#type: INPUT_MOUSE, ..Default::default() };
        mi.Anonymous.mi = MOUSEINPUT { dx: 1, dwFlags: MOUSEEVENTF_MOVE, dwExtraInfo: TAG, ..Default::default() };
        let mut ki = INPUT { r#type: INPUT_KEYBOARD, ..Default::default() };
        ki.Anonymous.ki = KEYBDINPUT { wScan: 0x76, dwFlags: KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP, dwExtraInfo: TAG, ..Default::default() };
        let sz = std::mem::size_of::<INPUT>() as i32;
        let s = Instant::now();
        let nm = unsafe { SendInput(&[mi], sz) };
        let em = windows::core::Error::from_thread();
        let nk = unsafe { SendInput(&[ki], sz) };
        let (mut lm, mut lk) = (None, None);
        while s.elapsed() < Duration::from_millis(2000) && (lm.is_none() || lk.is_none()) {
            if lm.is_none() && M.load(SeqCst) != m0 { lm = Some(s.elapsed()); }
            if lk.is_none() && K.load(SeqCst) != k0 { lk = Some(s.elapsed()); }
            std::thread::sleep(Duration::from_millis(1));
        }
        let mut p = POINT::default();
        unsafe { let _ = GetCursorPos(&mut p); }
        let fmt = |d: Option<Duration>| d.map_or("missed".to_string(), |d| format!("{}ms", d.as_millis()));
        let now = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        writeln!(f, "{:02}:{:02}:{:02}.{:03} sendM={nm} sendK={nk} err={:?} mouse={} key={} lastInput {li0}->{} (tick {tick0}->{}) cursor=({},{}) fg={}",
            now.wHour, now.wMinute, now.wSecond, now.wMilliseconds, em.code(), fmt(lm), fmt(lk),
            last_input(), unsafe { GetTickCount() }, p.x, p.y, fg()).unwrap();
        f.flush().unwrap();
        std::thread::sleep(Duration::from_secs(5).saturating_sub(s.elapsed()));
    }
    writeln!(f, "# done").unwrap();
}
