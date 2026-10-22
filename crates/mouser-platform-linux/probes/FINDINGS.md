# Linux hardware probes 4–6 (T427, T428): observed results

Everything below was observed. Where something could not be exercised it says so and why.

## Environment

All machines were aarch64 Lima VMs (Apple Virtualization.framework) on an Apple Silicon Mac, headless, no GPU. They are not the owner's x86_64 desktops, and there is no physical input hardware. Nothing here proves behaviour on real GPUs or seats, GDM/SDDM greeters, or a physical lock screen.

| VM | Distro / kernel | Desktop | xdg-desktop-portal | portal backend | libei |
|---|---|---|---|---|---|
| mouser-u2404 | Ubuntu 24.04.4, 6.8.0-124-generic | GNOME Shell 46.0 / mutter 46.2 (`gnome-shell --headless --wayland --no-x11 --virtual-monitor 1920x1080`) | 1.18.4 | xdg-desktop-portal-gnome 46.2 | 1.2.1 |
| mouser-u2604 | Ubuntu 26.04, 7.0.0-27-generic | GNOME Shell 50.1 / mutter 50.1 (same flags) | 1.21.1 | xdg-desktop-portal-gnome 50.0 | distro |
| mouser-f44kde | Fedora 44, 6.19.10-300.fc44 | KWin 6.7.5 (`kwin_wayland --virtual`) | 1.22.1 | xdg-desktop-portal-kde 6.7.5 | 1.6.0 |

Not run: GNOME 51 or Fedora 45 (no image template; Fedora 44 ships GNOME 50), and Plasma 6.8.

The Rust probes use ashpd 0.13.13 and reis 0.7.1 (pinned), built with `CARGO_TARGET_DIR=~/target-probes` inside each VM.

## How input was driven headless

- **GNOME:** mutter's own `org.gnome.Mutter.RemoteDesktop` D-Bus API (`scripts/drive.py`), which needs no portal and no dialog. Dialogs were approved with synthetic clicks. Screenshots came from `org.gnome.Mutter.ScreenCast`, PipeWire and GStreamer (`scripts/shot.py`).
- **KDE:** KWin's own EIS endpoint `org.kde.KWin.EIS.RemoteDesktop.connectToEIS`, driven by a reis sender (`scripts/kwin_eis.py`). KDE dialogs were approved blind with Enter. KWin's ScreenShot2 hung in the software-rendered virtual backend, so there are no Plasma screenshots.
- **X11:** Xvfb for grab tests; real Xorg (dummy video driver) with a kernel uinput mouse (`scripts/uinput_dev.py`, `scripts/xorg-start.sh`) for barrier tests.
- The portals' real dialogs were shown and answered on every desktop.

## Probe 4: portals (T427), `spike_portals/`

| Observation | GNOME 46 | GNOME 50 | Plasma 6.7.5 |
|---|---|---|---|
| RemoteDesktop interface / devices | v2, keyboard+pointer+touch | v2, same | v2, same |
| InputCapture interface | v1 (no `CreateSession2`/`Start`) | v1 | v2 |
| `SupportedCapabilities` raw | 15 (bit 8 set) | 15 | 7 |
| `host.portal.Registry` | absent (portal 1.18) | present; needs `com.mouser.app.desktop`; the dialog then says "The application Mouser" | present; `Register` ok with the desktop file |
| RemoteDesktop first Start | dialog with "Remember This Selection", "Allow Remote Interaction" toggle, and "Allow Clipboard Access" when requested; returns a restore token | same | dialog (approved blind); returns a token |
| RemoteDesktop persistence (same session, shell+portal restart, VM reboot) | silent, Start ~5 ms, same token every time | silent | silent; token changes after first use |
| RemoteDesktop clipboard | works only if the clipboard was part of the grant (`RequestClipboard` before `SelectDevices`). A token granted without it gives `clipboard_enabled=false` and `SetSelection` fails "Clipboard not enabled". With it, `SetSelection` is ok and `SelectionTransfer` is served | works; typed owner-changed stream works | works; typed stream works |
| InputCapture dialog | "Capture Input", toggle "Allow Remote Input Capturing", no "remember" | same | dialog (approved blind) |
| InputCapture persistence | impossible (v1) | impossible (v1) | `persist_mode=2` accepted but no restore token returned; a second Start re-prompted (timed out pending) |
| Clipboard on an InputCapture session | `RequestClipboard` gives AccessDenied "Invalid session type" | not requestable (v1) | works |
| Barrier accepted | `x+width` (y0..y1-1) accepted; `x+width-1` rejected (`failed_barriers=[1]`) | same | edge barrier accepted |
| Capture activation and events | `Activated` at (1920, 500); EIS delivers motion, key, button, scroll to a reis receiver | same | `Activated` at (2519, 500), not clamped to the zone; EIS delivers key, button, motion |
| Release | ok | ok | not exercised |

Replaying a stored RemoteDesktop token with different options (for example without the clipboard flag) raises a new dialog, so the options must match the original grant exactly.

## Probe 5: lock screen and drag (T428)

- **GNOME 46 and 50 lock screen:** with the screen locked, a persisted RemoteDesktop session started silently and an EIS absolute pointer moved the cursor on the lock screen.
  - On GNOME 46 a click on the password field's reveal icon changed its state.
  - Keyboard typing could not be verified: the headless unlock dialog has no PAM prompt without GDM.
  - On GNOME 50 the injected absolute x landed at 0.
  - On GNOME 50, replaying the token without the clipboard option raised a dialog that never resolved on the locked screen.
- **Plasma lock screen:** not exercised (`kwin_wayland --virtual --no-lockscreen`).
- **InputCapture during a drag (GNOME 46):** a GTK4 drag was started with the button held and the pointer pushed into the barrier. `Activated` was delivered mid-drag and the drag was cancelled (`GDK_DRAG_CANCEL_ERROR`, then `drag-end`). The later button release was rejected as an invalid event. Not run on GNOME 50 or Plasma.

## Probe 6: X11 (T428), `spike_lock_drag_x11/`

Run on Ubuntu 24.04 only. Ubuntu 26.04 has no X11 session, and Plasma X11 was not run.

- **Free grab:** with nothing else holding the grab, `XGrabPointer` and `XGrabKeyboard` succeed immediately.
- **Grab held by another client:** with another client holding the grab for 3 s, both returned `AlreadyGrabbed` until it released. With a 20 ms to 500 ms backoff the grab was acquired at 2657 ms.
- **Open menu:** with a real open GTK3 popup menu (`GDK_BACKEND=x11`), `AlreadyGrabbed` lasted for the entire time the menu was open (20 attempts over 8 s). It succeeded the moment the owner exited.
- **XFixes 5.0 barriers:** `directions` lists the directions allowed through, so blocking +x needs `NEGATIVE_X`.
  - **Edge barrier:** a vertical edge barrier at x=1000 stopped uinput motion from x=900 at 999. Motion back was free. `XWarpPointer` was not blocked (warped to 1200).
  - **XTest:** XTest motion is also blocked once the directions are right (stopped at 999).
  - **Corner:** in an L-shaped top-right corner with two barriers, straight-up and straight-right moves stopped at the barriers (y=200, x=1719). A diagonal move into the exact corner leaked through with 0 px and 2 px overlap (uinput, accelerated motion, ended at (1919, 0)) and was stopped with 20 px overlap ((1719, 200)).
  - **Not exercised:** XI2 `BarrierHit` events.

## Crate defects the implementation must work around

- `ashpd::InputCapture::supported_capabilities()` fails to decode on GNOME 46 and 50 (bit 8 set). Read the raw u32 instead.
- `remote_desktop::CreateSessionOptions` is private in ashpd 0.13.13; only `create_session(Default::default())` compiles.
- On GNOME 46 the typed `receive_selection_owner_changed` stream yields nothing although the raw signal arrives. GNOME 50 and Plasma are fine.
- After `ConnectToEIS`, the portal rejects `NotifyPointer*` and `NotifyKeyboard*`; injection must go through EIS.
- reis 0.7.1's event stream is not `Send`, so it needs its own thread with a current-thread runtime.

## Consequences for the plan

- **Persistence (T453):** `persist_mode=2` with a replaced restore token works for RemoteDesktop only. InputCapture has no persistence on GNOME 46/50, and none was observed on Plasma 6.7.5, so "Approve Mouser on <computer>" is the normal case after every login.
- **Clipboard on an InputCapture session:** needs Plasma 6.7+; it does not exist on GNOME 46/50. Clipboard must ride the RemoteDesktop session, requested up front.
- **Dialog blocking:** on Wayland, `capture()` can block for the approval dialog (6–40 s observed), so it must run off the caller's thread.
- **No "approval pending" hook:** the `mouser-platform` trait has no way to report "needs approval, pending"; surface it through `PlatformInfo.limitations` and `open_permission_settings`.
- **Drags:** InputCapture activation cancels an in-progress drag on GNOME 46, so dragging out of a Wayland computer by pushing into the edge does not work this way.

## Not exercised, and why

- GNOME 51 and Plasma 6.8: no image.
- Plasma lock screen, Plasma and GNOME 50 drag, KDE X11, GNOME 50 lock-screen keyboard: not run; the scripts are re-runnable.
- Anything needing a physical seat: GDM/SDDM greeters, real GPUs, real hotplug.
