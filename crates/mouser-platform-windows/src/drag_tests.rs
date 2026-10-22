use crate::drag::*;
use crate::dropdir::{desktop_folder, folder_at, folder_for, is_desktop_class};
use crate::edge::{Pending as EdgePending, THICKNESS, left_button_down, strips};
use crate::inject::{MOUSER_TAG, mouse_input, send};
use crate::vfiles::{Ended, descriptor_block, descriptor_name, new_data_object};
use mouser_platform::{DropTarget, Error, FileDrag};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Foundation::{
    DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DV_E_FORMATETC, DV_E_LINDEX, E_FAIL, HWND, POINT, RECT,
    S_OK,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};
use windows::Win32::System::Com::{
    DATADIR_GET, DATADIR_SET, DVASPECT_CONTENT, FORMATETC, IDataObject, TYMED_HGLOBAL,
    TYMED_ISTREAM,
};
use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_NONE, IDropTarget, OleInitialize};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
};
use windows::Win32::UI::Shell::{
    CFSTR_FILECONTENTS, CFSTR_FILEDESCRIPTORW, FILEDESCRIPTORW, SHCreateDataObject,
    SHParseDisplayName,
};
use windows::Win32::UI::Shell::{IShellWindows, IWebBrowserApp, ShellWindows};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, FindWindowW, GetSystemMetrics, GetWindowRect, IsWindowVisible,
    PostMessageW, SM_CXSCREEN, SM_CYSCREEN, SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow,
    SetWindowPos, WM_CLOSE, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetClassNameW, GetForegroundWindow, GetWindowTextW,
    GetWindowThreadProcessId, PhysicalToLogicalPoint, WindowFromPoint,
};
use windows::core::{Interface, PCWSTR, PWSTR, w};

/// Drags use the machine's one pointer and button, so tests that move it take
/// turns with every other test that does.
fn live() -> crate::LiveInput {
    crate::live_input()
}

fn ole() {
    unsafe { OleInitialize(None) }.unwrap();
}

// ---- geometry and small decisions ------------------------------------------------

#[test]
fn four_strips_run_along_the_outer_border_of_the_desktop() {
    let desktop = RECT {
        left: -1920,
        top: 0,
        right: 2560,
        bottom: 1440,
    };
    let [left, top, right, bottom] = strips(desktop);
    assert_eq!((left.left, left.right), (-1920, -1920 + THICKNESS));
    assert_eq!((right.left, right.right), (2560 - THICKNESS, 2560));
    assert_eq!((top.top, top.bottom), (0, THICKNESS));
    assert_eq!((bottom.top, bottom.bottom), (1440 - THICKNESS, 1440));
    assert_eq!((top.left, top.right), (-1920, 2560), "spans the corners");
    assert_eq!((left.top, left.bottom), (0, 1440));
}

#[test]
fn escape_cancels_a_released_button_drops_and_a_held_one_goes_on() {
    assert_eq!(continue_drag(true, true), DRAGDROP_S_CANCEL);
    assert_eq!(continue_drag(true, false), DRAGDROP_S_CANCEL);
    assert_eq!(continue_drag(false, false), DRAGDROP_S_DROP);
    assert_eq!(continue_drag(false, true), S_OK);
}

#[test]
fn a_drop_with_an_effect_is_dropped_and_other_endings_are_told_apart() {
    assert_eq!(
        ending(DRAGDROP_S_DROP, DROPEFFECT_COPY).unwrap(),
        Ending::Dropped
    );
    assert_eq!(
        ending(DRAGDROP_S_DROP, DROPEFFECT_NONE).unwrap(),
        Ending::Cancelled,
        "nobody took it"
    );
    assert_eq!(
        ending(DRAGDROP_S_CANCEL, DROPEFFECT_NONE).unwrap(),
        Ending::Cancelled
    );
    let Err(Error::Os(text)) = ending(E_FAIL, DROPEFFECT_NONE) else {
        panic!("a failure is an error")
    };
    assert!(text.starts_with("DoDragDrop failed"), "{text}");
}

#[test]
fn points_survive_being_packed_for_the_drop_source() {
    for (x, y) in [(0, 0), (2559, 1439), (-1920, 5), (7, -3), (-1, -1)] {
        assert_eq!(unpack(pack(POINT { x, y })), POINT { x, y });
    }
}

#[test]
fn the_desktop_has_two_window_classes() {
    assert!(is_desktop_class("Progman"));
    assert!(is_desktop_class("WorkerW"));
    assert!(!is_desktop_class("CabinetWClass"));
}

#[test]
fn the_desktop_class_maps_to_the_desktop_folder() {
    ole();
    let desktop = desktop_folder().unwrap();
    assert!(desktop.is_dir(), "{desktop:?}");
    assert_eq!(folder_for("Progman", HWND::default()).unwrap(), desktop);
    assert_eq!(folder_for("WorkerW", HWND::default()).unwrap(), desktop);
}

#[test]
fn a_window_that_is_not_explorer_is_not_a_folder() {
    ole();
    let Err(Error::Os(text)) = folder_for("Notepad", HWND::default()) else {
        panic!("not a folder")
    };
    assert!(text.contains("not a folder"), "{text}");
}

#[test]
fn files_waiting_at_an_edge_are_taken_once_and_cleared_on_leave() {
    let pending = EdgePending::default();
    assert_eq!(pending.take(Duration::ZERO), None);
    pending.set(vec![PathBuf::from(r"C:\a.txt")]);
    assert_eq!(
        pending.take(Duration::ZERO),
        Some(vec![PathBuf::from(r"C:\a.txt")])
    );
    assert_eq!(pending.take(Duration::ZERO), None);
    pending.set(vec![PathBuf::from(r"C:\b.txt")]);
    pending.clear();
    assert_eq!(pending.take(Duration::ZERO), None);
    pending.set(Vec::new());
    assert_eq!(
        pending.take(Duration::ZERO),
        None,
        "an empty list is nothing"
    );
}

#[test]
fn taking_waits_for_files_that_arrive_a_moment_later() {
    let pending = Arc::new(EdgePending::default());
    let setter = pending.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(80));
        setter.set(vec![PathBuf::from(r"C:\late.txt")]);
    });
    assert_eq!(
        pending.take(Duration::from_secs(5)),
        Some(vec![PathBuf::from(r"C:\late.txt")])
    );
    t.join().unwrap();
}

// ---- the virtual-file data object ------------------------------------------------

fn format(name: PCWSTR, medium: i32, index: i32) -> FORMATETC {
    FORMATETC {
        cfFormat: unsafe { RegisterClipboardFormatW(name) } as u16,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: index,
        tymed: medium as u32,
    }
}

fn names() -> Vec<String> {
    vec!["hello.txt".into(), "caf\u{e9}.png".into()]
}

#[test]
fn names_lose_anything_that_could_leave_the_folder_and_are_cut_to_fit() {
    let text = |n: &str| String::from_utf16(&descriptor_name(n)).unwrap();
    assert_eq!(text("plain.txt"), "plain.txt");
    assert_eq!(text(r"..\evil/x:y"), ".._evil_x_y");
    assert_eq!(text(r#"a*b?c"d<e>f|g"#), "a_b_c_d_e_f_g");
    assert_eq!(descriptor_name(&"x".repeat(400)).len(), 259);
}

#[test]
fn the_descriptor_block_counts_and_names_every_file() {
    let block = descriptor_block(&names()).unwrap();
    let at = unsafe { GlobalLock(block) }.cast::<u8>();
    let count = unsafe { at.cast::<u32>().read_unaligned() };
    assert_eq!(count, 2);
    let second = unsafe { at.add(4 + size_of::<FILEDESCRIPTORW>()) }.cast::<FILEDESCRIPTORW>();
    let name = unsafe { second.read_unaligned() }.cFileName;
    let len = name.iter().position(|&c| c == 0).unwrap();
    assert_eq!(String::from_utf16_lossy(&name[..len]), "caf\u{e9}.png");
    let _ = unsafe { GlobalUnlock(block) };
}

#[test]
fn the_data_object_offers_a_descriptor_and_empty_streams() {
    ole();
    let data = new_data_object(names(), Arc::new(Ended::default()));
    let desc = format(CFSTR_FILEDESCRIPTORW, TYMED_HGLOBAL.0, -1);
    let contents = |i| format(CFSTR_FILECONTENTS, TYMED_ISTREAM.0, i);
    assert_eq!(unsafe { data.QueryGetData(&desc) }, S_OK);
    assert_eq!(unsafe { data.QueryGetData(&contents(0)) }, S_OK);
    let wrong = format(w!("Text"), TYMED_HGLOBAL.0, -1);
    assert_eq!(unsafe { data.QueryGetData(&wrong) }, DV_E_FORMATETC);

    let medium = unsafe { data.GetData(&desc) }.unwrap();
    assert_eq!(medium.tymed, TYMED_HGLOBAL.0 as u32);
    let mut medium = medium;
    unsafe { windows::Win32::System::Ole::ReleaseStgMedium(&mut medium) };

    let medium = unsafe { data.GetData(&contents(1)) }.unwrap();
    assert_eq!(medium.tymed, TYMED_ISTREAM.0 as u32);
    let stream = unsafe { (*medium.u.pstm).clone() }.unwrap();
    let mut buf = [0u8; 8];
    let mut got = 0u32;
    let _ = unsafe { stream.Read(buf.as_mut_ptr().cast(), 8, Some(&mut got)) };
    assert_eq!(got, 0, "the placeholder is empty");

    assert_eq!(
        unsafe { data.GetData(&contents(2)) }.err().unwrap().code(),
        DV_E_LINDEX
    );
    assert_eq!(
        unsafe { data.GetData(&contents(-1)) }.err().unwrap().code(),
        DV_E_LINDEX
    );
    assert_eq!(
        unsafe { data.GetData(&wrong) }.err().unwrap().code(),
        DV_E_FORMATETC
    );
}

#[test]
fn the_rest_of_the_data_object_interface_answers_plainly() {
    ole();
    let data = new_data_object(names(), Arc::new(Ended::default()));
    let desc = format(CFSTR_FILEDESCRIPTORW, TYMED_HGLOBAL.0, -1);
    let mut medium = unsafe { data.GetData(&desc) }.unwrap();
    assert!(unsafe { data.GetDataHere(&desc, &mut medium) }.is_err());
    let mut canonical = desc;
    canonical.ptd = 1 as _;
    let hr = unsafe { data.GetCanonicalFormatEtc(&desc, &mut canonical) };
    assert_eq!(hr, windows::Win32::Foundation::DATA_S_SAMEFORMATETC);
    assert!(canonical.ptd.is_null());
    // The shell reports what it did; the object takes note and carries on.
    unsafe { data.SetData(&desc, &medium, true) }.unwrap();

    let formats = unsafe { data.EnumFormatEtc(DATADIR_GET.0 as u32) }.unwrap();
    let mut listed = [FORMATETC::default(); 4];
    let mut fetched = 0u32;
    let _ = unsafe { formats.Next(&mut listed, Some(&mut fetched)) };
    assert_eq!(fetched, 2);
    assert!(unsafe { data.EnumFormatEtc(DATADIR_SET.0 as u32) }.is_err());
    assert!(unsafe { data.DAdvise(&desc, 0, None) }.is_err());
    assert!(unsafe { data.DUnadvise(1) }.is_err());
    assert!(unsafe { data.EnumDAdvise() }.is_err());
}

#[test]
fn the_shells_asynchronous_copy_is_tracked_to_its_end() {
    ole();
    use windows::Win32::UI::Shell::IDataObjectAsyncCapability;
    use windows_core::Interface;
    let ended = Arc::new(Ended::default());
    let data = new_data_object(names(), ended.clone());
    let asynchronous: IDataObjectAsyncCapability = data.cast().unwrap();
    assert!(unsafe { asynchronous.GetAsyncMode() }.unwrap().as_bool());
    unsafe { asynchronous.SetAsyncMode(false) }.unwrap();
    assert!(!unsafe { asynchronous.GetAsyncMode() }.unwrap().as_bool());
    unsafe { asynchronous.SetAsyncMode(true) }.unwrap();

    assert!(!ended.started());
    assert!(!unsafe { asynchronous.InOperation() }.unwrap().as_bool());
    unsafe { asynchronous.StartOperation(None) }.unwrap();
    assert!(ended.started());
    assert!(unsafe { asynchronous.InOperation() }.unwrap().as_bool());
    assert_eq!(ended.wait(Duration::from_millis(20)), None);
    unsafe { asynchronous.EndOperation(S_OK, None, 1) }.unwrap();
    assert!(!unsafe { asynchronous.InOperation() }.unwrap().as_bool());
    assert_eq!(ended.wait(Duration::from_secs(1)), Some(S_OK));
}

fn physical<T>(f: impl FnOnce() -> T) -> T {
    crate::dpi::physical(f)
}

fn screen() -> (i32, i32) {
    physical(|| unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) })
}

fn send_mouse(inputs: &[INPUT]) {
    assert_eq!(
        unsafe { SendInput(inputs, size_of::<INPUT>() as i32) },
        inputs.len() as u32
    );
}

fn move_to(x: i32, y: i32) {
    let (w, h) = screen();
    send_mouse(&[INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx: x * 65535 / (w - 1),
                dy: y * 65535 / (h - 1),
                dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                dwExtraInfo: MOUSER_TAG,
                ..Default::default()
            },
        },
    }]);
}

fn button(down: bool) {
    send(&[mouse_input(
        if down {
            MOUSEEVENTF_LEFTDOWN
        } else {
            MOUSEEVENTF_LEFTUP
        },
        0,
        MOUSER_TAG,
    )])
    .unwrap();
}

fn glide(from: (i32, i32), to: (i32, i32)) {
    for s in 1..=20 {
        move_to(
            from.0 + (to.0 - from.0) * s / 20,
            from.1 + (to.1 - from.1) * s / 20,
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// A path in one comparable spelling: long names (the temp dir has an 8.3 short
/// form), no verbatim prefix, lower case.
fn same(path: &std::path::Path) -> String {
    let long = std::fs::canonicalize(path).unwrap();
    let text = long.to_string_lossy().to_lowercase();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_string()
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    done()
}

/// A shell data object for real files, as Explorer offers when files are
/// dragged.
fn shell_files(paths: &[PathBuf]) -> IDataObject {
    ole();
    let parse = |p: &std::path::Path| {
        let wide: Vec<u16> = p.to_string_lossy().encode_utf16().chain([0]).collect();
        let mut pidl = std::ptr::null_mut();
        unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None) }.unwrap();
        pidl.cast_const()
    };
    let folder = parse(paths[0].parent().unwrap());
    let children: Vec<*const windows::Win32::UI::Shell::Common::ITEMIDLIST> = paths
        .iter()
        .map(|p| unsafe { windows::Win32::UI::Shell::ILFindLastID(parse(p)) }.cast_const())
        .collect();
    unsafe { SHCreateDataObject(Some(folder), Some(&children), None::<&IDataObject>) }.unwrap()
}

/// Runs a drag of `data` on this thread, as the app showing the files would,
/// while `remote` plays the person (or remote computer) moving the pointer.
fn drag_while(data: IDataObject, remote: impl FnOnce() + Send + 'static) {
    ole();
    let pending = Pending {
        data,
        source: Source {
            at: Arc::new(DropPoint::default()),
        }
        .into(),
        result: Mutex::new(None),
    };
    let (hwnd, at) = helper_window(&pending);
    let (w, h) = screen();
    assert!(at.x >= 0 && at.x < w && at.y >= 0 && at.y < h);
    let player = std::thread::spawn(remote);
    button(true);
    let began = Instant::now();
    while pending.result.lock().unwrap().is_none() && began.elapsed() < Duration::from_secs(20) {
        pump();
        std::thread::sleep(Duration::from_millis(5));
    }
    player.join().unwrap();
    assert!(pending.result.lock().unwrap().is_some(), "the drag ended");
    let _ = unsafe { DestroyWindow(hwnd) };
}

/// Puts the pointer somewhere harmless and lets the pointer bookkeeping of
/// the system (the VM's mouse integration among it) settle, so the next test
/// that watches input starts from a quiet desktop.
fn settle() {
    let (w, h) = screen();
    move_to(w / 2, h / 2);
    std::thread::sleep(Duration::from_millis(500));
}

fn plain_window(rect: RECT) -> HWND {
    unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST,
            w!("STATIC"),
            w!("Mouser drag test"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            None,
            None,
        )
    }
    .unwrap()
}

// ---- the edge strips --------------------------------------------------------------

#[test]
fn the_strips_are_there_only_while_the_left_button_is_down() {
    let _live = live();
    let mut drag = WinFileDrag::new();
    let strips = drag.edge().strips();
    let visible = || {
        strips
            .iter()
            .all(|s| unsafe { IsWindowVisible(*s) }.as_bool())
    };
    let hidden = || {
        strips
            .iter()
            .all(|s| !unsafe { IsWindowVisible(*s) }.as_bool())
    };
    assert!(hidden());
    // Press on a window of our own so no other app gets the click.
    let (w, h) = screen();
    let window = plain_window(RECT {
        left: w / 2 - 150,
        top: h / 2 - 100,
        right: w / 2 + 150,
        bottom: h / 2 + 100,
    });
    move_to(w / 2, h / 2);
    std::thread::sleep(Duration::from_millis(200));
    button(true);
    assert!(wait_until(Duration::from_secs(3), visible));
    assert!(left_button_down());
    // With the button down, a drag that is late to the edge is waited for.
    let started = Instant::now();
    assert_eq!(drag.outgoing(), None);
    assert!(started.elapsed() >= Duration::from_millis(100));
    button(false);
    assert!(wait_until(Duration::from_secs(3), hidden));
    assert!(!left_button_down());
    // With it up, nobody waits.
    let started = Instant::now();
    assert_eq!(drag.outgoing(), None);
    assert!(started.elapsed() < Duration::from_millis(100));
    let _ = unsafe { DestroyWindow(window) };
    settle();
}

fn watcher() -> (IDropTarget, Arc<EdgePending>) {
    use crate::edge::Watcher;
    let pending = Arc::new(EdgePending::default());
    let target: IDropTarget = Watcher {
        pending: pending.clone(),
    }
    .into();
    (target, pending)
}

#[test]
fn a_strip_never_accepts_a_drop_and_forgets_what_it_saw() {
    use windows::Win32::Foundation::POINTL;
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    std::fs::write(&file, b"x").unwrap();
    let data = shell_files(std::slice::from_ref(&file));
    let (target, pending) = watcher();
    let (keys, at) = (MODIFIERKEYS_FLAGS(0), POINTL { x: 1, y: 2 });
    let mut effect = DROPEFFECT_COPY;
    unsafe { target.DragEnter(&data, keys, at, &mut effect) }.unwrap();
    assert_eq!(effect, DROPEFFECT_NONE);
    assert_eq!(
        pending.take(Duration::ZERO).map(|f| same(&f[0])),
        Some(same(&file))
    );

    effect = DROPEFFECT_COPY;
    unsafe { target.DragOver(keys, at, &mut effect) }.unwrap();
    assert_eq!(effect, DROPEFFECT_NONE);

    pending.set(vec![file.clone()]);
    unsafe { target.DragLeave() }.unwrap();
    assert_eq!(pending.take(Duration::ZERO), None, "left without crossing");

    pending.set(vec![file]);
    effect = DROPEFFECT_COPY;
    unsafe { target.Drop(&data, keys, at, &mut effect) }.unwrap();
    assert_eq!(effect, DROPEFFECT_NONE);
    assert_eq!(pending.take(Duration::ZERO), None);
}

#[test]
fn a_drag_without_a_file_list_is_not_a_file_drag() {
    use windows::Win32::Foundation::POINTL;
    use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
    ole();
    // Virtual files, as dragged in from another computer, have no paths.
    let data = new_data_object(vec!["x.txt".into()], Arc::new(Ended::default()));
    let (target, pending) = watcher();
    let mut effect = DROPEFFECT_COPY;
    unsafe { target.DragEnter(&data, MODIFIERKEYS_FLAGS(0), POINTL::default(), &mut effect) }
        .unwrap();
    assert_eq!(pending.take(Duration::ZERO), None);
}

#[test]
fn files_dragged_to_the_edge_are_handed_over() {
    let _live = live();
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<PathBuf> = ["one.txt", "two.txt"]
        .iter()
        .map(|n| {
            let p = dir.path().join(n);
            std::fs::write(&p, b"x").unwrap();
            p
        })
        .collect();
    let drag = Arc::new(Mutex::new(WinFileDrag::new()));
    let (w, h) = screen();
    move_to(w / 3, h / 3);
    std::thread::sleep(Duration::from_millis(200));
    let seen = Arc::new(Mutex::new(None));
    let (remote_drag, remote_seen) = (drag.clone(), seen.clone());
    drag_while(shell_files(&files), move || {
        std::thread::sleep(Duration::from_millis(500));
        glide((w / 3, h / 3), (w - 2, h / 3));
        std::thread::sleep(Duration::from_millis(300));
        *remote_seen.lock().unwrap() = remote_drag.lock().unwrap().outgoing();
        // A second ask finds nothing: the files are handed over once.
        assert_eq!(remote_drag.lock().unwrap().outgoing(), None);
        button(false);
    });
    let got: Vec<String> = seen
        .lock()
        .unwrap()
        .clone()
        .expect("the files at the edge")
        .iter()
        .map(|p| same(p))
        .collect();
    let want: Vec<String> = files.iter().map(|p| same(p)).collect();
    assert_eq!(got, want);
    drop(drag);
    settle();
}

// ---- starting a drag of files from another computer -------------------------------

#[test]
fn a_drag_of_no_files_is_refused() {
    let mut drag = WinFileDrag::new();
    assert_eq!(
        drag.begin_incoming(Vec::new()),
        Err(Error::Os("no files to drag".into()))
    );
}

#[test]
fn a_drag_whose_press_cannot_be_sent_is_an_error() {
    let _live = live();
    ole();
    let press = || Err(Error::Os("SendInput accepted 0 of 1 events".into()));
    assert_eq!(
        drag_with(vec!["a.txt".into()], &press, Duration::from_secs(5)),
        Err(Error::Os("SendInput accepted 0 of 1 events".into()))
    );
}

#[test]
fn a_drag_whose_press_never_arrives_gives_up_and_lets_go() {
    let _live = live();
    ole();
    let press = || Ok(());
    let started = Instant::now();
    let Err(Error::Os(text)) = drag_with(vec!["a.txt".into()], &press, Duration::from_millis(150))
    else {
        panic!("no drag")
    };
    assert!(text.contains("could not be started"), "{text}");
    assert!(started.elapsed() < Duration::from_secs(4));
    settle();
}

#[test]
fn how_the_drag_ended_decides_what_is_reported() {
    ole();
    use windows_core::Interface;
    let nowhere = POINT { x: 0, y: 0 };
    let ended = Arc::new(Ended::default());
    assert_eq!(
        conclude(Ok(Ending::Cancelled), &ended, nowhere),
        Err(Error::Os("drag cancelled".into()))
    );
    assert_eq!(
        conclude(Err(Error::Os("boom".into())), &ended, nowhere),
        Err(Error::Os("boom".into()))
    );
    // A drop: with no shell copy started there is nothing to wait for; with
    // one that has ended, the wait is over at once. Either way a folder is
    // looked for at the drop point (what is there does not matter here).
    let _ = conclude(Ok(Ending::Dropped), &ended, nowhere);
    let data = new_data_object(names(), ended.clone());
    let asynchronous: windows::Win32::UI::Shell::IDataObjectAsyncCapability = data.cast().unwrap();
    unsafe { asynchronous.StartOperation(None) }.unwrap();
    unsafe { asynchronous.EndOperation(S_OK, None, 1) }.unwrap();
    let started = Instant::now();
    let _ = conclude(Ok(Ending::Dropped), &ended, nowhere);
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[allow(clippy::zombie_processes)] // the Explorer it starts runs until its window is closed
fn explorer_window(dir: &std::path::Path) -> HWND {
    let name = dir.file_name().unwrap().to_string_lossy().into_owned();
    std::process::Command::new("explorer.exe")
        .arg(dir)
        .spawn()
        .unwrap();
    let title: Vec<u16> = format!("{name} - File Explorer")
        .encode_utf16()
        .chain([0])
        .collect();
    let mut found = None;
    assert!(
        wait_until(Duration::from_secs(20), || {
            found = unsafe { FindWindowW(w!("CabinetWClass"), PCWSTR(title.as_ptr())) }.ok();
            found.is_some()
        }),
        "Explorer opened {name}"
    );
    let window = found.unwrap();
    // Wholly on the screen, whatever its size: a drop point past the edge would
    // be clamped onto whatever is there instead of this window.
    let (w, h) = screen();
    let (left, top) = (w / 4, h / 8);
    let (width, height) = ((w - left - 20).min(1000), (h - top - 60).min(700));
    let _ = unsafe {
        SetWindowPos(
            window,
            None,
            left,
            top,
            width,
            height,
            SWP_NOZORDER | SWP_SHOWWINDOW,
        )
    };
    let _ = unsafe { SetForegroundWindow(window) };
    std::thread::sleep(Duration::from_millis(1500));
    window
}

// ---- what a failed drop reports ---------------------------------------------------

fn text_of(window: HWND) -> String {
    let mut title = [0u16; 128];
    let len = unsafe { GetWindowTextW(window, &mut title) }.max(0) as usize;
    String::from_utf16_lossy(&title[..len])
}

fn class_name(window: HWND) -> String {
    let mut name = [0u16; 64];
    let len = unsafe { GetClassNameW(window, &mut name) }.max(0) as usize;
    String::from_utf16_lossy(&name[..len])
}

fn image_of(pid: u32) -> String {
    let query = |process: HANDLE| {
        let mut path = [0u16; 520];
        let mut len = path.len() as u32;
        let name = PWSTR(path.as_mut_ptr());
        let got =
            unsafe { QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, name, &mut len) };
        let _ = unsafe { CloseHandle(process) };
        let text = |()| String::from_utf16_lossy(&path[..len as usize]);
        got.map_or_else(|e| format!("unreadable ({e})"), text)
    };
    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .map_or_else(|e| format!("unopenable ({e})"), query)
}

/// A window as a line: handle, class, title, rect, owning process and its integrity.
fn describe(window: HWND) -> String {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    let mut rect = RECT::default();
    let _ = physical(|| unsafe { GetWindowRect(window, &mut rect) });
    let integrity =
        crate::uipi::integrity_level(pid).map_or("unknown".into(), |l| format!("{l:#x}"));
    format!(
        "{:?} class {:?} title {:?} rect ({},{})-({},{}) pid {pid} {} integrity {integrity}",
        window.0,
        class_name(window),
        text_of(window),
        rect.left,
        rect.top,
        rect.right,
        rect.bottom,
        image_of(pid),
    )
}

fn names_in(dir: &std::path::Path) -> String {
    let list = |entries: std::fs::ReadDir| {
        let mut names: Vec<_> = entries.flatten().map(|e| e.file_name()).collect();
        names.sort();
        format!("{names:?}")
    };
    std::fs::read_dir(dir).map_or_else(|e| format!("unreadable ({e})"), list)
}

/// Which of the dropped files are in `dir` (a whole desktop is too long to print).
fn dropped_in(dir: &std::path::Path) -> String {
    let found = ["hello.txt", "big.bin"].map(|name| dir.join(name).exists());
    format!("hello.txt {}, big.bin {}", found[0], found[1])
}

/// Every `IShellWindows` entry: its window, and the folder it shows if that can be found.
fn shell_windows() -> String {
    let windows = unsafe { CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL) };
    let entry = |windows: &IShellWindows, i: i32| {
        let shown = unsafe { windows.Item(&VARIANT::from(i)) }
            .ok()
            .and_then(|item| unsafe { item.cast::<IWebBrowserApp>().ok()?.HWND() }.ok());
        let root = shown.map(|h| HWND(h.0 as *mut _));
        let folder = root.and_then(crate::dropdir::folder_of_explorer);
        format!("  [{i}] window {root:?} folder {folder:?}")
    };
    let all = |windows: IShellWindows| {
        let count = unsafe { windows.Count() }.unwrap_or(-1);
        let mut lines = vec![format!("IShellWindows count {count}")];
        lines.extend((0..count.max(0)).map(|i| entry(&windows, i)));
        lines.join("\n")
    };
    windows.map_or_else(|e| format!("IShellWindows unavailable ({e})"), all)
}

/// Everything worth knowing about why a drop did not land where it was aimed.
fn drop_report(
    explorer: HWND,
    dir: &std::path::Path,
    drop_at: (i32, i32),
    result: &mouser_platform::Result<DropTarget>,
) -> String {
    let (w, h) = screen();
    let at = POINT {
        x: drop_at.0,
        y: drop_at.1,
    };
    let mut logical = at;
    let _ = unsafe { PhysicalToLogicalPoint(explorer, &mut logical) };
    let under = physical(|| unsafe { WindowFromPoint(at) });
    let root = physical(|| unsafe { GetAncestor(under, GA_ROOT) });
    let own = unsafe { GetCurrentProcessId() };
    let foreground = unsafe { GetForegroundWindow() };
    let desktop = crate::dropdir::desktop_folder().ok();
    let lines = [
        format!("drop result: {result:?}"),
        format!(
            "drop point physical ({},{}) logical ({},{}) screen {w}x{h}",
            drop_at.0, drop_at.1, logical.x, logical.y
        ),
        format!("window at drop point: {}", describe(under)),
        format!("its root: {}", describe(root)),
        format!("explorer window: {}", describe(explorer)),
        format!(
            "explorer is the foreground window: {} (foreground: {})",
            foreground == explorer,
            describe(foreground)
        ),
        format!("the root is the explorer window: {}", root == explorer),
        format!(
            "test process pid {own} integrity {:?}",
            crate::uipi::integrity_level(own)
        ),
        shell_windows(),
        format!("files in the target folder: {}", names_in(dir)),
        format!(
            "dropped files on the desktop: {}",
            desktop.map_or("no desktop".into(), |d| dropped_in(&d))
        ),
    ];
    lines.join("\n")
}

#[test]
fn files_dragged_in_land_in_the_explorer_folder_they_were_dropped_on() {
    let _live = live();
    let base = tempfile::tempdir().unwrap();
    let dir = base
        .path()
        .join(format!("mouser-drop-{}", std::process::id()));
    std::fs::create_dir(&dir).unwrap();
    let explorer = explorer_window(&dir);
    // Physical pixels, like the drop point the drag reports (this thread may
    // be DPI-unaware, which would hand back scaled coordinates).
    let rect = physical(|| {
        let mut rect = RECT::default();
        unsafe { GetWindowRect(explorer, &mut rect) }.unwrap();
        rect
    });
    let drop_at = (
        rect.left + (rect.right - rect.left) * 7 / 10,
        rect.top + (rect.bottom - rect.top) * 6 / 10,
    );
    let start = (200, 300);
    move_to(start.0, start.1);
    std::thread::sleep(Duration::from_millis(200));
    let player = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(900));
        glide(start, drop_at);
        std::thread::sleep(Duration::from_millis(400));
        button(false);
    });
    let mut drag = WinFileDrag::new();
    let result = drag.begin_incoming(vec!["hello.txt".into(), "big.bin".into()]);
    player.join().unwrap();
    // Always gathered, so that what the failure message says is exercised too.
    let report = drop_report(explorer, &dir, drop_at, &result);
    eprintln!("{report}");
    let folder = result.as_ref().map(|t| t.0.clone());
    assert!(folder.is_ok(), "{result:?}\n{report}");
    let folder = folder.unwrap();
    assert!(
        same(&folder) == same(&dir),
        "{folder:?} is not {dir:?}\n{report}"
    );
    // The shell had made the (empty) files by the time the drag returned.
    for name in ["hello.txt", "big.bin"] {
        let meta = std::fs::metadata(dir.join(name));
        assert!(
            meta.is_ok_and(|m| m.len() == 0),
            "{name} is not a placeholder\n{report}"
        );
    }
    // The folder is found again from the same point while its window is there.
    let again = folder_at(POINT {
        x: drop_at.0,
        y: drop_at.1,
    });
    assert_eq!(same(&again.unwrap()), same(&dir));
    unsafe {
        PostMessageW(
            Some(explorer),
            WM_CLOSE,
            Default::default(),
            Default::default(),
        )
    }
    .unwrap();
    std::thread::sleep(Duration::from_millis(800));
    drop(drag);
    settle();
}
