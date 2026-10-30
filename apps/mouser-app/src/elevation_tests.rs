use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use mouser_core::logging::{FILE, Log};
use mouser_core::paths::Paths;

use crate::elevation::*;

/// What a fake Windows was asked, and how it answers.
#[derive(Default)]
struct FakeShell {
    /// The answer to a restart as administrator.
    answer: RefCell<Option<Result<(), RunAs>>>,
    /// The processes that are running.
    alive: Vec<u32>,
    asked: RefCell<Vec<String>>,
}

impl Shell for FakeShell {
    fn start(&self, exe: &Path, args: &str, as_admin: bool) -> Result<(), RunAs> {
        let verb = if as_admin { "runas" } else { "open" };
        self.asked
            .borrow_mut()
            .push(format!("{verb} {} {args}", exe.display()));
        self.answer.borrow_mut().take().expect("an answer")
    }

    fn wait_for_process(&self, pid: u32) {
        self.asked.borrow_mut().push(format!("wait for {pid}"));
    }

    fn process_alive(&self, pid: u32) -> bool {
        self.alive.contains(&pid)
    }

    fn message_box(&self, text: &str) {
        self.asked.borrow_mut().push(format!("say {text}"));
    }
}

fn answering(answer: Result<(), RunAs>) -> FakeShell {
    FakeShell {
        answer: RefCell::new(Some(answer)),
        ..FakeShell::default()
    }
}

/// A log in `dir`, and the elevation.restart outcomes it was given.
fn log_in(dir: &Path) -> Arc<Log> {
    let me = "00".repeat(16).parse().unwrap();
    Arc::new(Log::new(dir, me, "Alpha".into(), true, 1))
}

fn outcomes(dir: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(dir.join("logs").join(FILE)).unwrap_or_default();
    text.lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|l| l["event"] == "elevation.restart")
        .map(|l| l["fields"]["outcome"].as_str().unwrap().to_string())
        .collect()
}

fn portable(dir: &Path) -> Paths {
    Paths {
        dir: dir.to_path_buf(),
        portable: true,
    }
}

#[test]
fn an_accepted_restart_starts_this_exe_as_administrator_after_this_one_and_hands_over() {
    let dir = tempfile::tempdir().unwrap();
    let log = log_in(dir.path());
    let shell = answering(Ok(()));
    let exe = PathBuf::from(r"E:\Mouser\mouser-app.exe");
    let state = restart(&shell, &exe, 4242, &log);
    assert_eq!(state, State::HandingOver, "the caller shuts down and exits");
    assert_eq!(
        *shell.asked.borrow(),
        [r"runas E:\Mouser\mouser-app.exe --wait-for 4242"]
    );
    assert_eq!(outcomes(dir.path()), ["prompted", "handed_over"]);
}

#[test]
fn a_declined_prompt_changes_nothing_and_is_logged() {
    let dir = tempfile::tempdir().unwrap();
    let log = log_in(dir.path());
    let shell = answering(Err(RunAs::Declined));
    let state = restart(&shell, Path::new("mouser-app.exe"), 4242, &log);
    assert_eq!(
        state,
        State::Idle,
        "it runs on as before; the problem stays"
    );
    assert_eq!(outcomes(dir.path()), ["prompted", "declined"]);
}

#[test]
fn a_restart_windows_refuses_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let log = log_in(dir.path());
    let shell = answering(Err(RunAs::Failed("refused".into())));
    let state = restart(&shell, Path::new("mouser-app.exe"), 4242, &log);
    assert_eq!(state, State::Idle);
    assert_eq!(outcomes(dir.path()), ["prompted"]);
}

#[test]
fn an_elevated_portable_instance_marks_itself_while_it_runs() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(MARKER);
    let marker = Marker::write(&portable(dir.path()), true, 4242).expect("a marker");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "4242");
    drop(marker);
    assert!(!file.exists(), "deleted on exit");
}

#[test]
fn only_an_elevated_portable_instance_is_marked() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Marker::write(&portable(dir.path()), false, 4242).is_none());
    let installed = Paths {
        dir: dir.path().to_path_buf(),
        portable: false,
    };
    assert!(Marker::write(&installed, true, 4242).is_none());
    assert!(!dir.path().join(MARKER).exists());
}

#[test]
fn before_tauri_waits_for_the_instance_it_replaces() {
    let dir = tempfile::tempdir().unwrap();
    let shell = FakeShell::default();
    let args = ["mouser".to_string(), "--wait-for".into(), "4242".into()];
    assert!(!before_tauri(&args, &portable(dir.path()), true, &shell));
    assert_eq!(*shell.asked.borrow(), ["wait for 4242"]);
}

#[test]
fn a_second_launch_that_is_not_elevated_points_to_the_elevated_one_and_ends() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MARKER), "4242").unwrap();
    let shell = FakeShell {
        alive: vec![4242],
        ..FakeShell::default()
    };
    assert!(before_tauri(
        &["mouser".into()],
        &portable(dir.path()),
        false,
        &shell
    ));
    assert_eq!(
        *shell.asked.borrow(),
        [
            "say Mouser is already running as administrator on this computer. Open it from the tray icon."
        ]
    );
}

#[test]
fn a_stale_marker_or_an_elevated_launch_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(MARKER), "4242").unwrap();
    let gone = FakeShell::default();
    assert!(!before_tauri(
        &["mouser".into()],
        &portable(dir.path()),
        false,
        &gone
    ));
    let alive = FakeShell {
        alive: vec![4242],
        ..FakeShell::default()
    };
    assert!(!before_tauri(
        &["mouser".into()],
        &portable(dir.path()),
        true,
        &alive
    ));
    std::fs::write(dir.path().join(MARKER), "not a pid").unwrap();
    assert!(!before_tauri(
        &["mouser".into()],
        &portable(dir.path()),
        false,
        &alive
    ));
    assert!(gone.asked.borrow().is_empty() && alive.asked.borrow().is_empty());
}

#[test]
fn without_a_marker_a_launch_carries_on() {
    let dir = tempfile::tempdir().unwrap();
    let shell = FakeShell::default();
    assert!(!before_tauri(
        &["mouser".into()],
        &portable(dir.path()),
        false,
        &shell
    ));
}

#[test]
#[should_panic(expected = "--wait-for is followed by a process id")]
fn wait_for_without_a_pid_is_a_bug_in_the_caller() {
    let dir = tempfile::tempdir().unwrap();
    let args = ["mouser".to_string(), "--wait-for".into()];
    before_tauri(&args, &portable(dir.path()), true, &FakeShell::default());
}

#[test]
fn the_real_shell_answers_for_this_process() {
    let real = System;
    assert!(real.process_alive(std::process::id()));
    assert!(!real.process_alive(u32::MAX));
    real.wait_for_process(u32::MAX);
    // Not as administrator: tests never elevate.
    let missing = Path::new(r"C:\no\such\mouser-app.exe");
    assert!(matches!(
        real.start(missing, "", false),
        Err(RunAs::Failed(_))
    ));
}

#[test]
fn the_real_message_box_waits_until_it_is_closed() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let closer = std::thread::spawn(move || {
        let (class, title) = (wide("#32770"), wide("Mouser"));
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < end {
            let dialog = unsafe { FindWindowW(class.as_ptr(), title.as_ptr()) };
            if !dialog.is_null() {
                unsafe { PostMessageW(dialog, WM_CLOSE, 0, 0) };
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    });
    System.message_box("A test of the app's message box.");
    assert!(closer.join().unwrap(), "the box was shown and closed");
}
