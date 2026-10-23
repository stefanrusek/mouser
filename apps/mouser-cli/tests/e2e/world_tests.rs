//! Unit tests of `world.rs`, run by the `e2e_steps` test target.

use crate::world::task_action;

#[test]
fn a_desktop_task_runs_powershell_under_a_headless_console() {
    // A plain console program started by Task Scheduler is handed to Windows
    // Terminal (the default terminal), which brings an existing Terminal window
    // to the front and takes the foreground from the J1 note window.
    let action = task_action(r"C:\Users\Public\mouser-j1-note.ps1");
    assert_eq!(
        action,
        r"conhost.exe --headless powershell.exe -NoProfile -STA -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\Users\Public\mouser-j1-note.ps1"
    );
}
