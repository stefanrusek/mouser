//! J1 steps for Story 2: cross between the Mac (computer A, the controller)
//! and the Windows VM (computer B) by OS-level pointer movement on A, type on
//! B and read it back over SSH, and the panic key (quickstart.md section 2,
//! steps 5 and 6).
//!
//! Computer A's pointer and keyboard are driven by `input.rs`. Computer B shows
//! what arrived in the J1 note window (a tiny WinForms text box started in the user's
//! desktop session by a scheduled task, see `Computer::interactive`); Command-S
//! on A reaches the window as Ctrl-S, whose handler saves the text, and the file is read back over
//! SSH. Computer B is placed to the right of A by Mouser's automatic placement.

use cucumber::{given, then, when};

use crate::input;
use crate::us1_cli::status_where;
use crate::world::{Computer, J1World};

/// The file the note window writes on Ctrl+S on computer B.
const NOTE: &str = r"C:\Users\Public\mouser-j1-note.txt";

/// Far enough to cross any plausible pair of displays.
const FAR: i32 = 10_000;

/// `computer`'s own id, from its status.
pub(crate) fn id_of(computer: Computer) -> String {
    computer.mouser_json(&["status"])["computer"]["id"]
        .as_str()
        .expect("status names the computer")
        .to_string()
}

/// Wait until `controlled`'s keyboard and pointer are the desk's active computer.
pub(crate) fn wait_active(observer: Computer, controlled: Computer) {
    let id = id_of(controlled);
    status_where(observer, "the active computer to change", |s| {
        s["active_computer"] == id.as_str()
    });
}

/// PowerShell prelude for asking B's desktop session about windows.
const WIN32: &str = r#"Add-Type -Namespace J1 -Name W -MemberDefinition '
[DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
[DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
[DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
[DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);';
[void][J1.W]::SetProcessDPIAware();
function Title($h) { $s = New-Object System.Text.StringBuilder 512; [void][J1.W]::GetWindowText($h, $s, 512); $s.ToString() };
"#;

/// Where the note window leaves its handle and its process id.
const NOTE_HWND: &str = r"C:\Users\Public\mouser-j1-note.hwnd";
const NOTE_PID: &str = r"C:\Users\Public\mouser-j1-note.pid";

/// The J1-owned note window (a tiny WinForms app, see `j1-note.ps1`).
const NOTE_SCRIPT: &str = include_str!("../j1-note.ps1");

#[given("a plain text file is open in a note window on computer B")]
async fn note_open(_world: &mut J1World) {
    let cleared = Computer::B.script(&format!(
        "Remove-Item -Force -ErrorAction SilentlyContinue '{NOTE}', '{NOTE_HWND}', '{NOTE_PID}'"
    ));
    assert!(cleared.status.success(), "clear the note files on B");
    Computer::B.interactive("note", NOTE_SCRIPT);
    Computer::B.wait_for("the J1 note window to show on computer B", |c| {
        let out = c.script(&format!(
            "if ((Test-Path '{NOTE_HWND}') -and (Test-Path '{NOTE_PID}')) {{ exit 0 }} else {{ exit 3 }}"
        ));
        out.status.success().then_some(())
    });
    input::settle(2);
}

/// With the pointer on B, give the J1 note window keyboard focus by a real
/// click from A's mouse in the middle of B's display (the window is maximised),
/// then check the foreground window is the note.
pub(crate) fn focus_note() {
    let size = Computer::B.interactive_result(
        "screen",
        &format!(
            "{WIN32} '{{0}} {{1}}' -f [J1.W]::GetSystemMetrics(0), [J1.W]::GetSystemMetrics(1)"
        ),
    );
    let mut parts = size.split_whitespace().map(|n| n.parse::<i32>().unwrap());
    let (width, height) = (parts.next().unwrap(), parts.next().unwrap());
    // Overshoot into B's bottom-right corner (nothing is placed beyond it, so
    // the pointer clamps there), then come back a quarter of the way: inside
    // the maximised window, far from every edge Mouser treats as a crossing.
    input::move_pointer_by(width * 2, height * 2);
    input::move_pointer_by(-width / 4, -height / 4);
    input::click();
    input::settle(1);
    let title = Computer::B.interactive_result(
        "foreground",
        &format!("{WIN32} Title ([J1.W]::GetForegroundWindow())"),
    );
    assert!(
        title == "mouser-j1-note",
        "the foreground window on computer B is {title:?}, not the J1 note; typing would go \
         to the wrong window"
    );
}

#[when("the owner moves the pointer off the right edge of computer A's display")]
async fn cross_to_b(_world: &mut J1World) {
    input::push_pointer(FAR);
}

#[when("the owner moves the pointer off the left edge of computer B's display")]
async fn cross_to_a(_world: &mut J1World) {
    input::push_pointer(-FAR);
}

#[then("the pointer is on computer B")]
async fn on_b(_world: &mut J1World) {
    wait_active(Computer::A, Computer::B);
}

#[then("the pointer is back on computer A")]
async fn back_on_a(_world: &mut J1World) {
    wait_active(Computer::A, Computer::A);
}

#[when(expr = "the owner types {string} on computer A")]
async fn types(_world: &mut J1World, text: String) {
    focus_note();
    input::type_text(&text);
}

#[when("the owner saves with Command-S on computer A")]
async fn saves(_world: &mut J1World) {
    input::save();
}

#[then(expr = "the text file on computer B contains exactly {string}")]
async fn file_contains(_world: &mut J1World, text: String) {
    read_back(&text);
}

/// Wait for the note on B to equal `expected`, then close the note window
/// J1 started (only that window, by its handle, with a plain close request: the
/// file is already saved so there is no prompt) and remove the note.
pub(crate) fn read_back(expected: &str) {
    let found = Computer::B.wait_for("the typed text to be saved on computer B", |c| {
        let out = c.script(&format!(
            "[Console]::Out.Write([IO.File]::ReadAllText('{NOTE}'))"
        ));
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        (text == expected).then_some(text)
    });
    assert_eq!(found, expected);
    let closed = Computer::B.interactive_result(
        "close",
        &format!(
            "{WIN32} $h = [IntPtr][int64](Get-Content -Raw '{NOTE_HWND}'); \
             if ((Title $h) -eq 'mouser-j1-note') {{ \
               [void][J1.W]::PostMessage($h, 0x10, [IntPtr]::Zero, [IntPtr]::Zero); 'CLOSED' }} \
             else {{ 'GONE' }}"
        ),
    );
    assert!(
        closed == "CLOSED" || closed == "GONE",
        "close the J1 note window on B: {closed}"
    );
    Computer::B.wait_for("the J1 note window to exit on computer B", |c| {
        c.script(&format!(
            "if (Get-Process -Id ([int](Get-Content -Raw '{NOTE_PID}')) -ErrorAction SilentlyContinue) {{ exit 1 }}"
        ))
        .status
        .success()
        .then_some(())
    });
    let removed = Computer::B.script(&format!(
        "Remove-Item -Force '{NOTE}', '{NOTE_HWND}', '{NOTE_PID}'"
    ));
    assert!(removed.status.success(), "remove the note on B");
}

#[when("the owner presses Escape twice on computer A")]
async fn panic_key(_world: &mut J1World) {
    input::escape_twice();
}
