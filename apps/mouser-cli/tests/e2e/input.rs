//! Hardware-level input on computer A, the controller, for journey J1.
//!
//! Mechanism: `osascript -l JavaScript` (stock macOS) bridges to CoreGraphics and
//! posts mouse-move and key events at the HID event tap, the same level a
//! physical mouse and keyboard enter at, so Mouser's capture sees them exactly
//! as it sees real devices. Nothing is compiled or installed. The terminal (or
//! whatever launches the test) needs Accessibility permission to post events.

use std::time::Duration;

use crate::world::Computer;

/// The text typed for the continuous-typing check, one pangram per repeat.
/// 45 repeats at 30 ms per key is about 60 seconds of uninterrupted typing.
pub fn continuous_passage() -> String {
    "the quick brown fox jumps over the lazy dog ".repeat(45)
}

/// Pause between key presses, per key down and up (30 ms per character).
const KEY_MS: u32 = 15;

const PRELUDE: &str = "ObjC.import('ApplicationServices'); ObjC.import('Foundation');
function post(e) { $.CGEventPost($.kCGHIDEventTap, e); }
function pause(ms) { $.NSThread.sleepForTimeInterval(ms / 1000); }
function key(code, down, flags) {
  var e = $.CGEventCreateKeyboardEvent(null, code, down);
  if (flags) { $.CGEventSetFlags(e, flags); }
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
    script(&format!(
        "var codes = {{a:0,s:1,d:2,f:3,h:4,g:5,z:6,x:7,c:8,v:9,b:11,q:12,w:13,e:14,r:15,y:16,t:17,
  '1':18,'2':19,'3':20,'4':21,'6':22,'5':23,'9':25,'7':26,'8':28,'0':29,o:31,u:32,i:34,p:35,l:37,
  j:38,k:40,n:45,m:46,' ':49}};
var text = '{text}';
for (var i = 0; i < text.length; i++) {{ tap(codes[text[i]], 0); }}"
    ));
}

/// Press Command-S with real modifier key events.
pub fn save() {
    script(
        "key(55, true, $.kCGEventFlagMaskCommand);
pause(KEY_MS);
tap(1, $.kCGEventFlagMaskCommand);
key(55, false, 0);",
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
