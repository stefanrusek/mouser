//! J1 steps for Story 2: cross between the two Macs by OS-level pointer
//! movement on the controller (computer A), type on B and read it back over SSH,
//! and the panic key (quickstart.md section 2, steps 5 and 6).
//!
//! Computer A is the controller: its pointer and keyboard are driven by
//! `input.rs`. Computer B shows what arrived in a plain-text file open in
//! TextEdit; the file is read back with `cat` over SSH. Computer B is placed to
//! the right of A by Mouser's automatic placement (data-model.md).

use cucumber::{given, then, when};

use crate::input;
use crate::us1_cli::status_where;
use crate::world::{Computer, J1World};

/// The file TextEdit holds open on computer B.
pub(crate) const NOTE: &str = "/tmp/mouser-j1-note.txt";

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

#[given("a plain text file is open in TextEdit on computer B")]
async fn note_open(_world: &mut J1World) {
    let opened = Computer::B.run(
        "sh",
        &[
            "-c",
            &format!("printf '' > {NOTE} && open -a TextEdit {NOTE}"),
        ],
    );
    assert!(opened.status.success(), "open the note on B");
    input::settle(3);
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

/// Wait for the note on B to equal `expected`, then close TextEdit and remove it.
pub(crate) fn read_back(expected: &str) {
    let found = Computer::B.wait_for("the typed text to be saved on computer B", |c| {
        let out = c.run("cat", &[NOTE]);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        (text == expected).then_some(text)
    });
    assert_eq!(found, expected);
    let closed = Computer::B.run("osascript", &["-e", "quit app \"TextEdit\""]);
    assert!(closed.status.success(), "quit TextEdit on B");
    Computer::B.run("rm", &["-f", NOTE]);
}

#[when("the owner presses Escape twice on computer A")]
async fn panic_key(_world: &mut J1World) {
    input::escape_twice();
}
