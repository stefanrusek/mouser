//! Probe P3 (T014). Windows only. See the usage text (`spike_elevate help`).
//!
//! `single_instance_init` below copies, step for step, the Windows platform code of
//! tauri-plugin-single-instance 2.5.4 (`src/platform_impl/windows.rs`, lines 78-130 for the
//! second-instance branch): same name scheme (`{identifier}-sim` / `-sic` / `-siw`), the same
//! `CreateMutexW(NULL, TRUE, name)` followed by `GetLastError() == ERROR_ALREADY_EXISTS`, the same
//! `FindWindowW` + `AllowSetForegroundWindow` + `WM_COPYDATA` (dwData 1543, NUL separated
//! "cwd NUL arg0 NUL arg1 ...") + `SendMessageTimeoutW(SMTO_NORMAL, 10_000 ms)`, and the same hidden
//! window styles. Only logging (and `SetLastError(0)` before the send, so a zero result carries a
//! meaningful error) is added. The plugin's Tauri/AppHandle parts are not linked.
#![allow(clippy::all)]
#![allow(unsafe_op_in_unsafe_fn)]

use std::fs::File;
use std::io::Write;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows::core::{w, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Security::*;
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// The plugin uses `app.config().identifier` (apps/mouser-app/tauri.conf.json: "com.mouser.app").
/// The probe uses its own so it cannot collide with an installed Mouser.
const IDENTIFIER: &str = "com.mouser.spike-elevate";
const WMCOPYDATA_SINGLE_INSTANCE_DATA_NUL: usize = 1543;
const FORWARD_TIMEOUT_MS: u32 = 10_000;

static LOG: Mutex<Option<File>> = Mutex::new(None);

fn now_stamp() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = d.as_secs() as i64;
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // civil from days (Howard Hinnant)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dd = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y,
        m,
        dd,
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        d.subsec_millis()
    )
}

fn log(msg: impl AsRef<str>) {
    let line = format!("{} [pid {}] {}", now_stamp(), std::process::id(), msg.as_ref());
    println!("{line}");
    if let Some(f) = LOG.lock().unwrap().as_mut() {
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
}

fn open_log(mode: &str) {
    let dir = std::env::temp_dir().join("spike_elevate");
    let _ = std::fs::create_dir_all(&dir);
    let stamp = now_stamp().replace([':', '.'], "-");
    let path = dir.join(format!("{stamp}-pid{}-{mode}.log", std::process::id()));
    *LOG.lock().unwrap() = File::create(&path).ok();
    log(format!("log file {}", path.display()));
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// ---------- process facts ----------

unsafe fn token_facts(token: HANDLE) -> String {
    let mut elev = TOKEN_ELEVATION::default();
    let mut ret = 0u32;
    let e = GetTokenInformation(
        token,
        TokenElevation,
        Some(&mut elev as *mut _ as *mut _),
        std::mem::size_of::<TOKEN_ELEVATION>() as u32,
        &mut ret,
    );
    let elevated = if e.is_ok() { format!("{}", elev.TokenIsElevated != 0) } else { "?".into() };
    let mut need = 0u32;
    let _ = GetTokenInformation(token, TokenIntegrityLevel, None, 0, &mut need);
    let mut buf = vec![0u8; need.max(64) as usize];
    let il = if GetTokenInformation(
        token,
        TokenIntegrityLevel,
        Some(buf.as_mut_ptr() as *mut _),
        buf.len() as u32,
        &mut need,
    )
    .is_ok()
    {
        let tml = &*(buf.as_ptr() as *const TOKEN_MANDATORY_LABEL);
        let sid = tml.Label.Sid;
        let n = *GetSidSubAuthorityCount(sid) as u32;
        let rid = *GetSidSubAuthority(sid, n - 1);
        let name = match rid {
            0x0000 => "untrusted",
            0x1000 => "low",
            0x2000 => "medium",
            0x2100 => "medium-plus",
            0x3000 => "high",
            0x4000 => "system",
            _ => "other",
        };
        format!("{name} (0x{rid:X})")
    } else {
        "?".into()
    };
    format!("elevated={elevated} integrity={il}")
}

fn self_facts() -> String {
    unsafe {
        let mut tok = HANDLE::default();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok).is_err() {
            return "token unavailable".into();
        }
        let s = token_facts(tok);
        let _ = CloseHandle(tok);
        s
    }
}

fn process_facts(pid: u32) -> String {
    unsafe {
        let h = match OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) {
            Ok(h) => h,
            Err(e) => return format!("OpenProcess denied: {e}"),
        };
        let mut name = vec![0u16; 520];
        let mut n = name.len() as u32;
        let img = if QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(name.as_mut_ptr()), &mut n).is_ok() {
            String::from_utf16_lossy(&name[..n as usize])
        } else {
            "?".into()
        };
        let mut tok = HANDLE::default();
        let t = if OpenProcessToken(h, TOKEN_QUERY, &mut tok).is_ok() {
            let s = token_facts(tok);
            let _ = CloseHandle(tok);
            s
        } else {
            format!("OpenProcessToken denied (err {})", GetLastError().0)
        };
        let _ = CloseHandle(h);
        format!("{img}; {t}")
    }
}

// ---------- the plugin's single-instance init, replicated ----------

enum Init {
    /// Mutex created fresh: this is the first instance and owns the window (plugin's `else` branch).
    First { mutex: HANDLE, hwnd: HWND },
    /// Plugin would have forwarded and called `process::exit(0)`.
    Forwarded,
}

unsafe fn find_and_forward(class: &[u16], window: &[u16], who: &str) -> bool {
    let hwnd = FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(window.as_ptr())).unwrap_or_default();
    log(format!("{who}: FindWindowW -> {:?}", hwnd.0));
    if hwnd.0.is_null() {
        return false;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    log(format!("{who}: target window owner pid {pid}: {}", process_facts(pid)));
    if pid != 0 {
        let r = AllowSetForegroundWindow(pid);
        log(format!("{who}: AllowSetForegroundWindow -> ok={}", r.is_ok()));
    }
    let mut data = std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned();
    for arg in std::env::args_os() {
        data.push('\0');
        data.push_str(&arg.to_string_lossy());
    }
    let bytes = data.as_bytes();
    let cds = COPYDATASTRUCT {
        dwData: WMCOPYDATA_SINGLE_INSTANCE_DATA_NUL,
        cbData: bytes.len() as _,
        lpData: bytes.as_ptr() as _,
    };
    SetLastError(WIN32_ERROR(0)); // probe addition: so GetLastError after a 0 result is meaningful
    let sent = SendMessageTimeoutW(
        hwnd,
        WM_COPYDATA,
        WPARAM(0),
        LPARAM(&cds as *const _ as isize),
        SMTO_NORMAL,
        FORWARD_TIMEOUT_MS,
        None,
    );
    let err = GetLastError().0;
    if sent.0 == 0 {
        log(format!(
            "{who}: SendMessageTimeoutW(WM_COPYDATA) FAILED, result 0, GetLastError {err} ({}) -- plugin would warn 'failed to forward arguments'",
            if err == 5 {
                "ERROR_ACCESS_DENIED"
            } else if err == 1460 {
                "ERROR_TIMEOUT"
            } else {
                "other"
            }
        ));
    } else {
        log(format!("{who}: SendMessageTimeoutW(WM_COPYDATA) DELIVERED, result {}", sent.0));
    }
    true
}

unsafe fn single_instance_init(diagnostic_only: bool) -> Init {
    let class_name = wide(&format!("{IDENTIFIER}-sic"));
    let window_name = wide(&format!("{IDENTIFIER}-siw"));
    let mutex_name = wide(&format!("{IDENTIFIER}-sim"));

    SetLastError(WIN32_ERROR(0));
    let hmutex = CreateMutexW(None, true, PCWSTR(mutex_name.as_ptr()));
    let last = GetLastError();
    let (mutex, mutex_err) = match &hmutex {
        Ok(h) => (*h, None),
        Err(e) => (HANDLE::default(), Some(e.to_string())),
    };
    log(format!(
        "CreateMutexW('{IDENTIFIER}-sim', initial_owner=TRUE) -> handle {:?}{}; GetLastError {} ({})",
        mutex.0,
        mutex_err.map(|e| format!(" (windows-rs Err: {e})")).unwrap_or_default(),
        last.0,
        if last == ERROR_ALREADY_EXISTS {
            "ERROR_ALREADY_EXISTS"
        } else if last.0 == 5 {
            "ERROR_ACCESS_DENIED"
        } else if last.0 == 0 {
            "fresh"
        } else {
            "other"
        }
    ));

    if last == ERROR_ALREADY_EXISTS {
        // plugin: forward to the existing window, cleanup, process::exit(0)
        if !find_and_forward(&class_name, &window_name, "second") {
            log("second: mutex exists but no target window found (plugin would then fall through and continue starting as a normal instance)");
        }
        if !mutex.0.is_null() {
            let _ = CloseHandle(mutex);
        }
        return Init::Forwarded;
    }
    if diagnostic_only {
        // Not the plugin: the plugin's `else` branch would create its own window and run as a second
        // full instance (this is what a NULL mutex handle / ACCESS_DENIED leads to). Probe the window
        // path directly instead.
        log("second: mutex was NOT reported as ERROR_ALREADY_EXISTS; plugin's else-branch would run a FULL second instance here. Diagnostic: trying the WM_COPYDATA path anyway.");
        let found = find_and_forward(&class_name, &window_name, "second");
        if !found {
            log("second: no running instance window found");
        }
        if !mutex.0.is_null() {
            let _ = ReleaseMutex(mutex);
            let _ = CloseHandle(mutex);
        }
        return Init::Forwarded;
    }
    let hwnd = create_event_target_window(&class_name, &window_name);
    log(format!("first instance: window created {:?}", hwnd.0));
    Init::First { mutex, hwnd }
}

unsafe fn destroy(mutex: HANDLE, hwnd: HWND) {
    if !mutex.0.is_null() {
        let _ = ReleaseMutex(mutex);
        let _ = CloseHandle(mutex);
    }
    let _ = DestroyWindow(hwnd);
    log("mutex released, window destroyed");
}

const TIMER_SECOND: usize = 1;
const TIMER_INJECT: usize = 2;
const TIMER_HANDOVER: usize = 3;
const TIMER_RELAUNCH: usize = 4;
const TIMER_EXIT: usize = 5;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_COPYDATA => {
            let cds = &*(lparam.0 as *const COPYDATASTRUCT);
            if cds.dwData != WMCOPYDATA_SINGLE_INSTANCE_DATA_NUL {
                return LRESULT(1);
            }
            let bytes = if cds.lpData.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize)
            };
            let data = String::from_utf8_lossy(bytes).replace('\0', " | ");
            log(format!("*** WM_COPYDATA RECEIVED from second launch: cwd|args = {data}"));
            LRESULT(1)
        }
        WM_TIMER => {
            on_timer(hwnd, wparam.0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn create_event_target_window(class_name: &[u16], window_name: &[u16]) -> HWND {
    let hinst = GetModuleHandleW(None).unwrap_or_default();
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: hinst.into(),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };
    RegisterClassExW(&class);
    let hwnd = CreateWindowExW(
        WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
        PCWSTR(class_name.as_ptr()),
        PCWSTR(window_name.as_ptr()),
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
    .unwrap_or_default();
    SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_VISIBLE | WS_POPUP).0 as isize);
    hwnd
}

// ---------- hooks ----------

static MOVES: AtomicU32 = AtomicU32::new(0);

unsafe extern "system" fn kb_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let k = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let injected = k.flags.0 & 0x10 != 0;
        let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
        if down {
            log(format!(
                "HOOK key down vk=0x{:02X} scan=0x{:02X} injected={injected}",
                k.vkCode, k.scanCode
            ));
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe extern "system" fn ms_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let m = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        let injected = m.flags & 1 != 0;
        match wparam.0 as u32 {
            WM_MOUSEMOVE => {
                MOVES.fetch_add(1, Ordering::Relaxed);
                if injected {
                    log(format!("HOOK mouse move injected=true at ({},{})", m.pt.x, m.pt.y));
                }
            }
            other => log(format!(
                "HOOK mouse msg=0x{other:X} injected={injected} at ({},{})",
                m.pt.x, m.pt.y
            )),
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

// ---------- injection ----------

unsafe fn inject() {
    let fg = GetForegroundWindow();
    let mut pid = 0u32;
    GetWindowThreadProcessId(fg, Some(&mut pid));
    log(format!("INJECT: foreground window {:?} pid {pid}: {}", fg.0, process_facts(pid)));
    log(format!("INJECT: injector: {}", self_facts()));

    let mut inputs: Vec<INPUT> = Vec::new();
    for ch in "SPIKE-ELEVATE-OK".encode_utf16() {
        for up in [false, true] {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: ch,
                        dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            });
        }
    }
    SetLastError(WIN32_ERROR(0));
    let n = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    log(format!(
        "INJECT: SendInput(text 'SPIKE-ELEVATE-OK', {} events) returned {n}, GetLastError {} (UIPI refusals are silent)",
        inputs.len(),
        GetLastError().0
    ));

    // a 1px relative mouse move and back (no clicks are injected)
    let mv = |dx: i32| INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dx, dy: 0, mouseData: 0, dwFlags: MOUSEEVENTF_MOVE, time: 0, dwExtraInfo: 0 },
        },
    };
    let n = SendInput(&[mv(1), mv(-1)], std::mem::size_of::<INPUT>() as i32);
    log(format!("INJECT: SendInput(mouse move +1,-1) returned {n}"));
}

// ---------- relaunch ----------

fn quote(a: &str) -> String {
    if a.is_empty() || a.contains(' ') || a.contains('"') {
        format!("\"{}\"", a.replace('"', "\\\""))
    } else {
        a.to_string()
    }
}

/// ShellExecuteExW(verb="runas", exe, "run --wait-for <own pid> <forward...>"). Returns true if started.
fn relaunch(wait_for: bool, forward: &[String]) -> bool {
    unsafe {
        let exe = std::env::current_exe().unwrap();
        let mut params = String::from("run");
        if wait_for {
            params.push_str(&format!(" --wait-for {}", std::process::id()));
        }
        for a in forward {
            params.push(' ');
            params.push_str(&quote(a));
        }
        log(format!(
            "RELAUNCH: ShellExecuteExW(verb=runas, file={}, params={params}) -- UAC prompt expected",
            exe.display()
        ));
        let file = wide(&exe.to_string_lossy());
        let pars = wide(&params);
        let mut sei = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
            lpVerb: w!("runas"),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(pars.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        SetLastError(WIN32_ERROR(0));
        let r = ShellExecuteExW(&mut sei);
        let err = GetLastError().0;
        match r {
            Ok(()) => {
                let newpid = if sei.hProcess.0.is_null() { 0 } else { GetProcessId(sei.hProcess) };
                log(format!("RELAUNCH: ShellExecuteExW OK (UAC accepted); new process pid {newpid}"));
                if !sei.hProcess.0.is_null() {
                    let _ = CloseHandle(sei.hProcess);
                }
                true
            }
            Err(e) => {
                log(format!(
                    "RELAUNCH: ShellExecuteExW FAILED: {e}; GetLastError {err}{}",
                    if err == 1223 { " == ERROR_CANCELLED (UAC declined)" } else { "" }
                ));
                false
            }
        }
    }
}

// ---------- --wait-for ----------

fn wait_for(pid: u32) {
    unsafe {
        let t0 = Instant::now();
        log(format!("--wait-for {pid}: opening process (SYNCHRONIZE) before touching the single-instance mutex"));
        match OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            Err(e) => log(format!("--wait-for: OpenProcess failed ({e}); treating pid as already gone")),
            Ok(h) => {
                let r = WaitForSingleObject(h, 60_000);
                log(format!(
                    "--wait-for: WaitForSingleObject -> {} ({}) after {} ms",
                    r.0,
                    if r == WAIT_OBJECT_0 {
                        "parent exited"
                    } else if r == WAIT_TIMEOUT {
                        "TIMEOUT, parent still alive"
                    } else {
                        "other"
                    },
                    t0.elapsed().as_millis()
                ));
                let _ = CloseHandle(h);
            }
        }
    }
}

// ---------- state & main ----------

struct Opts {
    wait_for: Option<u32>,
    inject_after: Option<u32>,
    inject_count: u32,
    exit_after: Option<u32>,
    relaunch_after: Option<u32>,
    relaunch_nowait: bool,
    no_hooks: bool,
    forward: Vec<String>,
}

static INJECT_LEFT: AtomicU32 = AtomicU32::new(0);
static FORWARD: Mutex<Vec<String>> = Mutex::new(Vec::new());
static NOWAIT: AtomicU32 = AtomicU32::new(0);

unsafe fn on_timer(hwnd: HWND, id: usize) {
    match id {
        TIMER_SECOND => {
            let m = MOVES.swap(0, Ordering::Relaxed);
            if m > 0 {
                log(format!("HOOK mouse: {m} move events in the last second"));
            }
        }
        TIMER_INJECT => {
            inject();
            let left = INJECT_LEFT.fetch_sub(1, Ordering::Relaxed).saturating_sub(1);
            if left == 0 {
                let _ = KillTimer(Some(hwnd), TIMER_INJECT);
            }
        }
        TIMER_RELAUNCH => {
            let _ = KillTimer(Some(hwnd), TIMER_RELAUNCH);
            let fwd = FORWARD.lock().unwrap().clone();
            if relaunch(NOWAIT.load(Ordering::Relaxed) == 0, &fwd) {
                log("RELAUNCH accepted: old instance keeps the mutex for 3 s (child must wait), then exits");
                SetTimer(Some(hwnd), TIMER_HANDOVER, 3000, None);
            } else {
                log("RELAUNCH not performed: old instance keeps running unchanged");
            }
        }
        TIMER_HANDOVER => {
            let _ = KillTimer(Some(hwnd), TIMER_HANDOVER);
            log("old instance: shutting down now (mutex released just before exit)");
            PostQuitMessage(0);
        }
        TIMER_EXIT => {
            log("--exit-after reached");
            PostQuitMessage(0);
        }
        _ => {}
    }
}

fn parse(args: &[String]) -> Opts {
    let mut o = Opts {
        wait_for: None,
        inject_after: None,
        inject_count: 3,
        exit_after: None,
        relaunch_after: None,
        relaunch_nowait: false,
        no_hooks: false,
        forward: vec![],
    };
    let mut i = 0;
    let num = |i: usize| -> u32 {
        args.get(i).and_then(|s| s.parse::<u32>().ok()).unwrap_or_else(|| {
            eprintln!("bad number for {}", args[i - 1]);
            std::process::exit(2)
        })
    };
    while i < args.len() {
        match args[i].as_str() {
            "--wait-for" => {
                i += 1;
                o.wait_for = Some(num(i));
            }
            "--inject-after" => {
                i += 1;
                o.inject_after = Some(num(i));
            }
            "--inject-count" => {
                i += 1;
                o.inject_count = num(i);
            }
            "--exit-after" => {
                i += 1;
                o.exit_after = Some(num(i));
            }
            "--relaunch-after" => {
                i += 1;
                o.relaunch_after = Some(num(i));
            }
            "--relaunch-nowait" => o.relaunch_nowait = true,
            "--no-hooks" => o.no_hooks = true,
            "--" => {
                o.forward = args[i + 1..].to_vec();
                break;
            }
            other => {
                eprintln!("unknown option {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    o
}

const USAGE: &str = "spike_elevate (probe P3)
  run  [--wait-for PID] [--inject-after S [--inject-count N]] [--exit-after S] [--relaunch-after S [--relaunch-nowait]] [--no-hooks] [-- args forwarded to the relaunched child]
       Takes the single-instance mutex + hidden window exactly as tauri-plugin-single-instance 2.5.4 does, logs integrity,
       installs WH_KEYBOARD_LL/WH_MOUSE_LL and logs events, optionally injects text + a 1px mouse move into the foreground,
       optionally does the runas relaunch (this instance holds the mutex, relaunches elevated with --wait-for <own pid>, exits 3 s later).
       If the mutex already exists it forwards WM_COPYDATA like the plugin and exits 0.
  relaunch [--relaunch-nowait] [-- child args]
       ShellExecuteExW(runas) of 'run --wait-for <own pid> ...' and exit at once. Logs ERROR_CANCELLED (1223) if declined.
  second
       Plugin-style second launch only: mutex, FindWindowW, AllowSetForegroundWindow, WM_COPYDATA; logs the outcome and exits.
       Also tries the window path when the mutex did not report ERROR_ALREADY_EXISTS (diagnostic).
Logs: %TEMP%\\spike_elevate\\<timestamp>-pid<pid>-<mode>.log";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let mode = argv.first().map(String::as_str).unwrap_or("help").to_string();
    if !matches!(mode.as_str(), "run" | "relaunch" | "second") {
        println!("{USAGE}");
        return;
    }
    open_log(&mode);
    log(format!("mode={mode} args={:?} exe={}", &argv[1..], std::env::current_exe().unwrap().display()));
    log(format!("self: {}", self_facts()));
    let o = parse(&argv[1..]);

    if mode == "relaunch" {
        let ok = relaunch(!o.relaunch_nowait, &o.forward);
        log(format!("relaunch mode done (started={ok}); exiting"));
        std::process::exit(if ok { 0 } else { 1 });
    }

    if let Some(pid) = o.wait_for {
        wait_for(pid);
    }

    unsafe {
        if mode == "second" {
            let _ = single_instance_init(true);
            log("second: done");
            return;
        }
        let (mutex, hwnd) = match single_instance_init(false) {
            Init::Forwarded => {
                log("plugin behaviour: second instance forwarded its arguments and exits with code 0");
                std::process::exit(0);
            }
            Init::First { mutex, hwnd } => (mutex, hwnd),
        };
        log(format!("RUNNING as the first instance. {}", self_facts()));

        let (mut hk, mut hm) = (None, None);
        if !o.no_hooks {
            let hmod = GetModuleHandleW(None).unwrap_or_default();
            match SetWindowsHookExW(WH_KEYBOARD_LL, Some(kb_hook), Some(hmod.into()), 0) {
                Ok(h) => {
                    log("WH_KEYBOARD_LL installed");
                    hk = Some(h)
                }
                Err(e) => log(format!("WH_KEYBOARD_LL FAILED: {e}")),
            }
            match SetWindowsHookExW(WH_MOUSE_LL, Some(ms_hook), Some(hmod.into()), 0) {
                Ok(h) => {
                    log("WH_MOUSE_LL installed");
                    hm = Some(h)
                }
                Err(e) => log(format!("WH_MOUSE_LL FAILED: {e}")),
            }
        }

        *FORWARD.lock().unwrap() = o.forward.clone();
        NOWAIT.store(o.relaunch_nowait as u32, Ordering::Relaxed);
        SetTimer(Some(hwnd), TIMER_SECOND, 1000, None);
        if let Some(s) = o.inject_after {
            INJECT_LEFT.store(o.inject_count, Ordering::Relaxed);
            log(format!(
                "will inject {} time(s), every {s} s, first in {s} s: put the target app in the foreground",
                o.inject_count
            ));
            SetTimer(Some(hwnd), TIMER_INJECT, s * 1000, None);
        }
        if let Some(s) = o.relaunch_after {
            log(format!("will relaunch elevated in {s} s"));
            SetTimer(Some(hwnd), TIMER_RELAUNCH, s * 1000, None);
        }
        if let Some(s) = o.exit_after {
            SetTimer(Some(hwnd), TIMER_EXIT, s * 1000, None);
        }

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if let Some(h) = hk {
            let _ = UnhookWindowsHookEx(h);
        }
        if let Some(h) = hm {
            let _ = UnhookWindowsHookEx(h);
        }
        destroy(mutex, hwnd);
        log("exit");
    }
}
