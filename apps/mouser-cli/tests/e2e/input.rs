//! Hardware-level input on computer A, the controller, for journey J1.
//!
//! Mechanism: `osascript -l JavaScript` (stock macOS) bridges to CoreGraphics and
//! posts mouse and key events at the HID event tap, the same level a physical
//! mouse and keyboard enter at, so Mouser's capture tap (also at the HID tap)
//! sees them exactly as it sees real devices. Every key event is built the way
//! Mouser's own injector builds them (crates/mouser-platform-macos/src/inject.rs):
//! from an `HIDSystemState` source, with its modifier flags set explicitly as a
//! hardware keyboard reports them (a modifier key is a flags-changed event that
//! carries its device left/right bit, not only the device-independent flag), so
//! nothing inherits whatever modifier state the system had. AppleScript's System
//! Events `keystroke` is never used: it posts at the session tap, after Mouser's
//! HID tap, so Mouser never sees it and it goes to the Mac's frontmost app.
//! Nothing is compiled or installed. The terminal (or whatever launches the
//! test) needs Accessibility permission to post events.

use std::time::Duration;

use crate::world::Computer;

/// The text typed for the continuous-typing check, one pangram per repeat.
/// 45 repeats at 30 ms per key is about 60 seconds of uninterrupted typing.
pub fn continuous_passage() -> String {
    "the quick brown fox jumps over the lazy dog ".repeat(45)
}

/// Pause between key presses, per key down and up (30 ms per character).
const KEY_MS: u32 = 15;

/// `kCGEventFlagMaskCommand | NX_DEVICELCMDKEYMASK`: what a hardware keyboard
/// reports while the left Command key is down.
const LEFT_COMMAND_DOWN: u32 = 0x0010_0008;

/// Numeric CoreGraphics constants (JXA exposes some named ones as strings):
/// `kCGEventSourceStateHIDSystemState` 1, `kCGHIDEventTap` 0,
/// `kCGEventFlagsChanged` 12, `kCGEventFlagMaskNonCoalesced` 0x100.
const PRELUDE: &str = "ObjC.import('ApplicationServices'); ObjC.import('Foundation');
var SRC = $.CGEventSourceCreate(1);
function post(e) { $.CGEventPost(0, e); }
function pause(ms) { $.NSThread.sleepForTimeInterval(ms / 1000); }
function key(code, down, flags) {
  var e = $.CGEventCreateKeyboardEvent(SRC, code, down);
  $.CGEventSetFlags(e, flags | 0x100);
  post(e);
}
function modifier(code, down, flags) {
  var e = $.CGEventCreateKeyboardEvent(SRC, code, down);
  $.CGEventSetType(e, 12);
  $.CGEventSetFlags(e, flags | 0x100);
  post(e);
}
function tap(code, flags) { key(code, true, flags); pause(KEY_MS); key(code, false, flags); pause(KEY_MS); }
";

fn script(body: &str) {
    let source = format!("var KEY_MS = {KEY_MS};\n{PRELUDE}{body}");
    let out = Computer::A.run("osascript", &["-l", "JavaScript", "-e", &source]);
    assert!(
        out.status.success(),
        "posting input on computer A failed (does the terminal have Accessibility permission?): {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Push the pointer horizontally by `distance` points (positive is right) in
/// 40-point steps, 8 ms apart, along a line 100 points below the top of the
/// display (computer B's displays are placed to the right of A's, top-aligned).
pub fn push_pointer(distance: i32) {
    let step = 40 * distance.signum();
    let steps = distance.abs() / 40;
    script(&format!(
        "var here = $.CGEventGetLocation($.CGEventCreate(null));
$.CGWarpMouseCursorPosition({{x: here.x, y: 100}});
for (var i = 0; i < {steps}; i++) {{
  var at = $.CGEventGetLocation($.CGEventCreate(null));
  var e = $.CGEventCreateMouseEvent(null, $.kCGEventMouseMoved, {{x: at.x + {step}, y: 100}}, 0);
  $.CGEventSetIntegerValueField(e, $.kCGMouseEventDeltaX, {step});
  $.CGEventSetIntegerValueField(e, $.kCGMouseEventDeltaY, 0);
  post(e);
  pause(8);
}}"
    ));
}

/// Type `text` (lowercase letters, digits and spaces of the ANSI layout).
pub fn type_text(text: &str) {
    assert!(
        text.chars()
            .all(|c| c == ' ' || c.is_ascii_lowercase() || c.is_ascii_digit()),
        "only lowercase letters, digits and spaces can be typed: {text:?}"
    );
    assert_no_secure_input();
    script(&format!(
        "var codes = {{a:0,s:1,d:2,f:3,h:4,g:5,z:6,x:7,c:8,v:9,b:11,q:12,w:13,e:14,r:15,y:16,t:17,
  '1':18,'2':19,'3':20,'4':21,'6':22,'5':23,'9':25,'7':26,'8':28,'0':29,o:31,u:32,i:34,p:35,l:37,
  j:38,k:40,n:45,m:46,' ':49}};
var text = '{text}';
for (var i = 0; i < text.length; i++) {{ tap(codes[text[i]], 0); }}"
    ));
}

/// Move the pointer by (`dx`, `dy`) points relative to where it is, in steps of
/// at most 40 points, 8 ms apart. No warp: while the pointer is on computer B
/// these deltas are what Mouser forwards.
pub fn move_pointer_by(dx: i32, dy: i32) {
    let steps = (dx.abs().max(dy.abs()) / 40).max(1);
    let (sx, sy) = (dx / steps, dy / steps);
    script(&format!(
        "for (var i = 0; i < {steps}; i++) {{
  var at = $.CGEventGetLocation($.CGEventCreate(null));
  var e = $.CGEventCreateMouseEvent(null, $.kCGEventMouseMoved, {{x: at.x + {sx}, y: at.y + {sy}}}, 0);
  $.CGEventSetIntegerValueField(e, $.kCGMouseEventDeltaX, {sx});
  $.CGEventSetIntegerValueField(e, $.kCGMouseEventDeltaY, {sy});
  post(e);
  pause(8);
}}"
    ));
}

/// Click the left button where the pointer is.
pub fn click() {
    script(
        "var at = $.CGEventGetLocation($.CGEventCreate(null));
post($.CGEventCreateMouseEvent(null, $.kCGEventLeftMouseDown, at, 0));
pause(KEY_MS);
post($.CGEventCreateMouseEvent(null, $.kCGEventLeftMouseUp, at, 0));
pause(KEY_MS);",
    );
}

/// Press Command-S as a keyboard does: left Command down (a flags-changed
/// event carrying Command and its left-key bit), S down and up with those
/// flags, Command up with no flags. Mouser forwards Command as Control to B.
pub fn save() {
    assert_no_secure_input();
    script(&format!(
        "modifier(55, true, {LEFT_COMMAND_DOWN});
pause(KEY_MS);
tap(1, {LEFT_COMMAND_DOWN});
modifier(55, false, 0);
pause(KEY_MS);"
    ));
}

/// Fail fast when Secure Input is on on computer A: the OS then hides every key
/// from event taps, Mouser's included, so typed keys would go to the Mac's
/// frontmost app instead of computer B (pointer events are unaffected).
fn assert_no_secure_input() {
    let out = Computer::A.run("ioreg", &["-l", "-w", "0"]);
    assert!(out.status.success(), "ioreg on computer A failed");
    let holder = String::from_utf8_lossy(&out.stdout)
        .lines()
        .find(|line| line.contains("kCGSSessionSecureInputPID"))
        .map(str::to_string);
    assert!(
        holder.is_none(),
        "Secure Input is on on computer A, so Mouser cannot see typed keys: {}",
        holder.unwrap_or_default().trim()
    );
}

/// Press Escape twice in quick succession (the default panic key, `Escape*2`).
pub fn escape_twice() {
    script("tap(53, 0); pause(60); tap(53, 0);");
}

/// Let the other computer settle between steps.
pub fn settle(seconds: u64) {
    std::thread::sleep(Duration::from_secs(seconds));
}
