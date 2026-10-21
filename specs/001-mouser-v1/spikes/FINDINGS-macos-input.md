# Spike findings: macOS input capture, injection, Secure Input, displays, full-screen

Plan-phase empirical spike for 001-mouser-v1 (Stories 2, 5, 11; Plan-Phase Handoff). Throwaway probe code only. Saved by the plan orchestrator from the probe agent's report.

## Environment and versions
- macOS 26.5 (25F71), arm64. rustc 1.99.0 (b940084d7 2026-09-28).
- Probe: `specs/001-mouser-v1/spikes/spike_macos_input/` (one binary with subcommands).
- Crates (pinned `=`): `core-graphics =0.25.0`, `core-foundation =0.10.1`; transitive core-foundation-sys 0.8.7, core-graphics-types 0.2.0, foreign-types 0.5.0, libc 0.2.191, bitflags 2.13.2.
- Raw FFI (core-graphics 0.25 lacks wrappers): AXIsProcessTrusted; CGPreflightListenEventAccess, CGPreflightPostEventAccess, CGPreflightScreenCaptureAccess, CGSessionCopyCurrentDictionary, CGGetEventTapList, CGRectMakeWithDictionaryRepresentation; Carbon IsSecureEventInputEnabled/EnableSecureEventInput/DisableSecureEventInput; IOKit IOHIDCheckAccess; dlsym.
- core-graphics 0.25 `CGEventSource` has no `set_user_data`; use `CGEvent::set_integer_value_field(EVENT_SOURCE_USER_DATA, ..)`.
- Build/run: `CARGO_TARGET_DIR=<scratchpad>/spike-target cargo build`, then `$CARGO_TARGET_DIR/debug/spike_macos_input <perms|listen|listen-fields|active|confine|inject|secure|secure-self|taps|displays|fullscreen>`.
- Safety: watchdog hard-exits each subcommand within 5 s; taps listen-only except `active` (filters MouseMoved only, 1.5 s); no keystrokes injected; mouse moves 7–30 pt with cursor restored.
- **Permission context:** process chain probe → zsh → claude → zsh → Warp.app (pid 905). macOS attributes TCC to the responsible app (Warp), which holds Accessibility, Input Monitoring and Screen Recording, so nothing was blocked. A fresh unsigned Mouser binary would see false/Denied until granted.
- Other input software running: ShareMouse (pid 20080), BetterTouchTool, BetterSnapTool, FluidVoice.

## 1. Listen-only CGEventTap; permission preflight
- **Assumption:** listen-only tap for mouse moves and keys can be created; AXIsProcessTrusted / CGPreflightListenEventAccess report state without prompting.
- **Probe:** main.rs perms(), listen(), listen_fields(). **Command:** `spike_macos_input perms | listen | listen-fields`
- **Observed:** AXIsProcessTrusted=true; CGPreflightListenEventAccess=true; CGPreflightPostEventAccess=true; IOHIDCheckAccess(ListenEvent)=Granted, (PostEvent)=Granted; CGPreflightScreenCaptureAccess=true. "listen tap mouse@Session / mouse@HID / keys@Session / keys@HID: created OK; events received in 1s = 0" (user idle). In probe 3 a listen tap received this process's own injected moves.
- **Verdict:** Confirmed with permission granted (inherited from Warp). Denied path not exercised.
- **Implication:** use non-prompting preflight calls for UI/CLI status; call prompting request APIs only from an explicit "grant" action. TCC grants belong to the responsible bundle — test with a signed .app with no grants. Follow-up: re-run from a host with no grants.

## 2. Active (filtering) tap; edge confinement
- **Assumption:** a filtering tap can swallow events; warp + disassociate + hide can freeze the local cursor while Mouser reads motion.
- **Probe:** main.rs active(), confine(), taps(). **Command:** `spike_macos_input active | confine | taps`
- **Observed:** "active tap: created OK; MouseMoved swallowed in 1.5s = 1; cursor before=(693.07,568.08) after_injected_move=(700.07,575.08)" — swallowed but the cursor still moved. confine: warp to centre Ok (1280,720); Associate(false) Ok; HideCursor Ok; "while disassociated 1.0s: tap_ok=true moves_seen=1 sum|dx|=20 cursor=(1300,720)" (injected deltaX=20); ShowCursor Ok; Associate(true) Ok; restore warp Ok (693,568). CGGetEventTapList found 23 taps; ShareMouse has one HID filtering tap, mask 0xee5c5cfe (mouse down/up/move/drag, Key down/up/flags, NX_SYSDEFINED, gestures, scroll, other-mouse). BetterTouchTool also runs HID filtering taps. Tap-list latency fields implausible — unreliable.
- **Verdict:** Confirmed, with one assumption refuted: dropping MouseMoved does not stop the cursor; a filtering tap only hides the move from apps.
- **Implication:** sender-side remote mode = one HID filtering tap dropping keys/buttons/scroll/gestures/NX_SYSDEFINED; `CGAssociateMouseAndMouseCursorPosition(false)`; park cursor via warp (optionally hide); read kCGMouseEventDeltaX/Y. Not verified with a real mouse; not verified that CGDisplayHideCursor hides when Mouser is not frontmost (historically needs a private property or a frontmost transparent window). Tap callback must be minimal and re-enable on TapDisabledByTimeout. Do not run Mouser and ShareMouse simultaneously during testing.

## 3. Injection
- **Assumption:** CGEventPost works when post access is granted; a modern low-level path exists for apps that ignore CGEvents.
- **Probe:** main.rs inject(), virtual_hid_probe(). **Command:** `spike_macos_input inject`
- **Observed:** "HIDSystemState source -> kCGHIDEventTap: moved=true"; "CombinedSessionState -> kCGSessionEventTap: moved=true"; "Private -> kCGHIDEventTap: moved=true" (cursor restored). Observer tap saw srcPID=31634 (self) for all three sources; srcStateID 1, 0, random; userData=0x4D4F55534552 preserved. dlsym found IOHIDPostEvent, IOHIDUserDeviceCreate, IOHIDUserDeviceCreateWithProperties, IOHIDUserDeviceHandleReportWithTimeStamp, CGEventPostToPid, CGEventPostToPSN. "virtual HID: IOHIDUserDeviceCreateWithProperties returned NULL". Absolute CGEventPost moved the cursor even while disassociated.
- **Verdict:** CGEventPost confirmed. Low-level path for apps ignoring CGEvents: refuted for an unentitled binary.
- **Implication:** injected CGEvents carry the injector's pid (field 41) regardless of source — detectable. kCGEventSourceUserData (field 42) is preserved: tag Mouser's own events to avoid feedback loops. IOHIDPostEvent deprecated — do not use. Story 2 sc. 7 on macOS needs a virtual HID device (DriverKit HIDDriverKit virtual device, as Karabiner does, or IOHIDUserDevice), expected to need restricted entitlements (inferred from the NULL result, not confirmed from Apple docs). **Risk:** baseline is CGEventPost (HIDSystemState source at kCGHIDEventTap); virtual-HID path gated separately.

## 4. Secure Input
- **Assumption:** IsSecureEventInputEnabled() detects Secure Input; owner PID available via CGSessionCopyCurrentDictionary or ioreg.
- **Probe:** main.rs secure(), secure_self() (enables Secure Input <1 s). **Command:** `spike_macos_input secure | secure-self`; `ioreg -l -w 0 | grep -i SecureInput`
- **Observed:** idle: false; kCGSSessionSecureInputPID absent from session dict; ioreg count 0. With probe pid 30875 enabling it: true; kCGSSessionSecureInputPID=905 (/Applications/Warp.app/Contents/MacOS/stable); ioreg agrees. After disable: false.
- **Verdict:** Confirmed; both sources agree; key absent (not 0) when off.
- **Implication:** poll IsSecureEventInputEnabled(); read PID from CGSessionCopyCurrentDictionary (no ioreg spawn). PID is the responsible app (fits "name the application"; names the terminal for CLI tools). Map pid → name via NSRunningApplication, fallback proc_pidpath. Keys are undocumented: fall back to a generic message. Secure Input is a normal state, not a disconnect.

## 5. Displays
- **Assumption:** CG display APIs give layout, pixel size, scale and physical size.
- **Probe:** main.rs displays(). **Command:** `spike_macos_input displays`
- **Observed:** id=3 main, vendor 0x1e6d model 0x5b08 (5K): bounds (0,0) 2560x1440 pt; CGDisplayPixelsWide/High 2560x1440; mode 2560x1440 pt / 5120x2880 px, scale 2, 60 Hz; 599.3x340.2 mm, 217 ppi, 4.272 pt/mm. id=2 vendor 0x4c2d model 0x730b (34" ultrawide): bounds (-880,-1440) 3440x1440; scale 1, 50 Hz; 801.6x338.7 mm, 109 ppi, 4.291 pt/mm.
- **Verdict:** Confirmed.
- **Implication:** CGDisplayPixelsWide returns points; use CGDisplayModeGetPixelWidth/GetWidth for scale. Global coordinates in points, origin top-left of main display, y down, may be negative. Physical speed via bounds.width / CGDisplayScreenSize.width; EDID size can be 0 or wrong (fallback needed). Stable identity from vendor/model/serial (CGDirectDisplayID is not stable). Use CGDisplayRegisterReconfigurationCallback for changes (not probed).

## 6. Full-screen detection
- **Assumption:** comparing CGWindowListCopyWindowInfo bounds with display bounds detects a full-screen frontmost app.
- **Probe:** main.rs fullscreen(). **Command:** `spike_macos_input fullscreen`
- **Observed:** 67 on-screen windows; Window Server "Menubar" layer 24 (0,0) 2560x30; front layer-0 window Warp (pid 905) (71,118) 2235x1235, covers no display; names readable (Screen Recording granted); one Menubar with two displays.
- **Verdict:** Partially confirmed — data available; positive case not exercised.
- **Implication:** frontmost pid via NSWorkspace.frontmostApplication → its first layer-0 window on the cursor's display; full-screen if bounds equal display bounds within 1 pt; also any window at layer ≥ CGShieldingWindowLevel covering the display (CGDisplayCapture games). Missing Menubar is weak corroboration. Alternative: AXFullScreen on the focused window.

## Open follow-ups
1. Re-run perms/listen from a host with no grants to record denied-tap behaviour.
2. With a human moving the mouse, verify deltas arrive while disassociated and HideCursor works when Mouser is not frontmost.
3. Determine the virtual-HID entitlement path (blocks Story 2 sc. 7 on macOS).
4. Test full-screen detection with a native full-screen app, a borderless game and a CGDisplayCapture game.
