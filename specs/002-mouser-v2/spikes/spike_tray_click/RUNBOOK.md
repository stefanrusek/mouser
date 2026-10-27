# P4 runbook: spike_tray_click (T015)

The spike only observes. It injects no input, grabs nothing, and never touches the clipboard. You click by hand.
Run it twice: on this Mac, then on the Parallels VM (Windows). Build on each machine first.

## Launch

```
cargo run --manifest-path specs/002-mouser-v2/spikes/spike_tray_click/Cargo.toml
```

(or run the built `spike_tray_click` binary from `specs/002-mouser-v2/spikes/spike_tray_click/target/debug/`). A tray icon appears with no window. The runbook is printed to the terminal. The log is `spike_tray_click.log` in the same directory as the binary (`.../target/debug/spike_tray_click.log`); the path is printed on the `start` line. Delete an old log before the second machine's run, or note the `os=` field.

## Steps (about 1 second between clicks, do not double-click)

1. Left-click the tray icon 3 times. Each release should swap the icon badge (watch for flicker or a blank frame).
2. Right-click the tray icon 2 times. The menu should open; press Escape or click away to close it.
3. Open the menu (right-click) and choose "Menu item (log only)".
4. Open the menu and choose "Quit spike". The process exits and logs `exit`.
5. Send back `spike_tray_click.log` and note, for each machine, whether the icon visibly flickered on swap and whether a left click ever opened the menu.

On Windows the icon may be hidden under the notification-area overflow arrow; drag it out to the bar so clicks are easy.

## Criteria (record each in FINDINGS.md as confirmed or refuted, per OS)

1. Left vs right reported with `show_menu_on_left_click(false)`:
   - Confirmed: left clicks log `TrayIconEvent::Click button=Left`, right clicks log `button=Right`, and a left click does not open the menu (you saw no menu on left click). Right click opens the menu.
   - Refuted: a button is logged wrongly or as another variant, or left click opens the menu.
2. `button_state: Up` once per click:
   - Confirmed: for each of the 5 clicks, exactly one `Click ... button_state=Up` line (and typically one `Down`). Expected counts: 3 Left/Up, 2 Right/Up.
   - Refuted: zero or more than one Up for any click (record the actual counts; note if the right-click Up is missing because the menu swallowed it, which matters for tray handling).
3. Runtime `set_icon` swaps badges without flicker:
   - Confirmed: each Left/Up is followed by a `set_icon variant=... result=Ok(())` line, variants alternate A, B, A, and you saw the badge change each time with no flicker, blank frame, or icon disappearing.
   - Refuted: `result=Err(..)`, the badge does not change, or visible flicker/disappearance.

Also record anything surprising in the log (extra Enter/Move/Leave/DoubleClick variants, positions, time gaps).
