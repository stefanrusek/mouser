//! Steps for `tests/features/v2_us1_guard.feature` (Story 1 sc. 6).
//!
//! "The pointer is on another desk computer" is a helper process, standing
//! in for Mouser, that holds a real capture grabbed: the system arrow is
//! blank and the capture's real guard is running. "Mouser ends
//! unexpectedly" is `TerminateProcess` of the helper. "The pointer is
//! visible" is the backend's own query, `pointer_hidden() == false`, and the
//! guard ending by itself, well.

use cucumber::{World, given, then, when};
use mouser_platform::Platform;
use mouser_platform_windows::{WinPlatform, cursor_guard, pointer_hidden};
use std::io::{BufRead, BufReader};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::process::{Child, Command, ExitCode, Stdio};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    WaitForSingleObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_SETCURSORS, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
};

/// The argument that makes this binary the helper parent.
const HELPER: &str = "--helper-parent";
/// What the helper prints once its capture is grabbed.
const GRABBED: &str = "grabbed";

/// The part this run of the binary plays, other than the suite: the guard,
/// started by the helper's capture, or the helper itself. `None` for the
/// suite.
pub fn role(args: &[String]) -> Option<ExitCode> {
    match args.get(1).map(String::as_str) {
        Some("--cursor-guard") => {
            let parent = args[2].parse().expect("the parent's pid");
            Some(cursor_guard::run(parent))
        }
        Some(HELPER) => helper_parent(),
        _ => None,
    }
}

/// Stands in for Mouser with the pointer on another computer: a real
/// capture, grabbed, until the process is ended from outside.
fn helper_parent() -> ! {
    let platform = WinPlatform::new();
    let mut capture = platform.capture().expect("a capture");
    let (tx, _rx) = std::sync::mpsc::channel();
    capture.start(tx).expect("the capture starts");
    capture.set_grabbed(true).expect("the capture grabs");
    println!("{GRABBED}");
    loop {
        std::thread::park();
    }
}

#[derive(Debug, Default, World)]
pub struct GuardWorld {
    helper: Option<Helper>,
    /// The helper's guard, opened before the helper is ended.
    guard: Option<OwnedHandle>,
}

/// The helper parent. Dropped, it is ended and waited for, and the user's
/// cursors are reloaded if they are still blank, so a failed scenario
/// leaves neither behind.
#[derive(Debug)]
struct Helper(Child);

impl Drop for Helper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        if pointer_hidden() {
            let _ = unsafe {
                SystemParametersInfoW(
                    SPI_SETCURSORS,
                    0,
                    None,
                    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
                )
            };
        }
    }
}

/// Where the helper and its guard write coverage data: not the coverage
/// run's own files, since the helper is killed mid-run and would leave a
/// broken one there.
fn throwaway_profile() -> std::path::PathBuf {
    std::env::temp_dir().join("mouser-acc-guard-%p.profraw")
}

/// The processes `parent` started that are this binary.
fn children_of(parent: u32) -> Vec<u32> {
    let exe = std::env::current_exe().unwrap();
    let own = exe.file_name().unwrap().to_string_lossy().to_lowercase();
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.unwrap();
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut found = Vec::new();
    let mut listed = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while listed {
        let file = &entry.szExeFile;
        let len = file.iter().position(|&c| c == 0).unwrap_or(file.len());
        let name = String::from_utf16_lossy(&file[..len]).to_lowercase();
        if entry.th32ParentProcessID == parent && name == own {
            found.push(entry.th32ProcessID);
        }
        listed = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    let _ = unsafe { windows::Win32::Foundation::CloseHandle(snapshot) };
    found
}

#[given("the pointer is on another desk computer")]
fn pointer_on_another_computer(world: &mut GuardWorld) {
    assert!(
        !pointer_hidden(),
        "the pointer is shown before the scenario"
    );
    let mut child = Command::new(std::env::current_exe().unwrap())
        .arg(HELPER)
        .env("LLVM_PROFILE_FILE", throwaway_profile())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    world.helper = Some(Helper(child));
    let mut line = String::new();
    BufReader::new(stdout).read_line(&mut line).unwrap();
    assert_eq!(line.trim(), GRABBED, "the helper grabbed its capture");
    assert!(pointer_hidden(), "the grab hid the pointer");
}

#[when("Mouser on the computer the desk user is at ends unexpectedly")]
fn mouser_ends_unexpectedly(world: &mut GuardWorld) {
    let helper = world.helper.as_mut().expect("a helper");
    let guards = children_of(helper.0.id());
    assert_eq!(guards.len(), 1, "the capture started one guard");
    let access = PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION;
    let guard = unsafe { OpenProcess(access, false, guards[0]) }.unwrap();
    world.guard = Some(unsafe { OwnedHandle::from_raw_handle(guard.0) });
    // `kill` is TerminateProcess: no clean-up runs in the helper.
    helper.0.kill().unwrap();
    helper.0.wait().unwrap();
}

#[then("the pointer is visible on that computer")]
fn pointer_visible(world: &mut GuardWorld) {
    let end = Instant::now() + Duration::from_secs(5);
    while pointer_hidden() && Instant::now() < end {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!pointer_hidden(), "the guard brought the pointer back");
    let guard = HANDLE(world.guard.as_ref().expect("the guard").as_raw_handle());
    let ended = unsafe { WaitForSingleObject(guard, 5_000) } == WAIT_OBJECT_0;
    assert!(ended, "the guard ended by itself");
    let mut code = 1;
    unsafe { GetExitCodeProcess(guard, &mut code) }.unwrap();
    assert_eq!(code, 0, "the guard ended well");
    for leftover in std::fs::read_dir(std::env::temp_dir()).unwrap().flatten() {
        let name = leftover.file_name().to_string_lossy().into_owned();
        if name.starts_with("mouser-acc-guard-") && name.ends_with(".profraw") {
            let _ = std::fs::remove_file(leftover.path());
        }
    }
}
