# Configuration

Mouser keeps its settings in one plain-text file, `config.toml`. Every setting in the app's windows is a key in this file, and nothing else is needed to configure Mouser. You can edit the file by hand, use `mouser config get|set` (see [cli.md](cli.md)), or use the app. Most people never touch any of it.

## Where the files are

| OS | Config folder |
|---|---|
| macOS | `~/Library/Application Support/Mouser/` |
| Linux | `$XDG_CONFIG_HOME/mouser/`, or `~/.config/mouser/` when `XDG_CONFIG_HOME` is not set |
| Windows | `%APPDATA%\Mouser\` (there is no working Windows backend in v1) |

`mouser config path` prints the file's full path. Inside the folder:

| Path | What it is |
|---|---|
| `config.toml` | The settings (this page). |
| `state.json` | Which desk this computer is on, the desk's shared layout and the last displays seen. Written by Mouser; do not edit it. |
| `logs/mouser.log`, `logs/mouser.log.1` | The diagnostic log, only while logging is on (see [troubleshooting.md](troubleshooting.md)). |
| `clipboard/<transfer id>/` | Files received through the clipboard. Not cleaned up automatically. |
| `releases/` | Update bundles this computer downloaded and shares with the desk. |
| `mouser.sock` (macOS) | The local control socket the `mouser` command uses. |

Other places:

- The control socket on Linux is `$XDG_RUNTIME_DIR/mouser.sock`; on Windows it is the named pipe `\\.\pipe\mouser-<USERNAME>`.
- Secrets (this computer's identity key and each desk's key) are not in the config folder: macOS keeps them in the Keychain; Linux uses the desktop's Secret Service when one is unlocked, otherwise 0600 files in `<config folder>/secrets/`.

If the config folder or `config.toml` does not exist, Mouser creates them with the defaults on first start.

## Portable mode

Put a folder named `MouserData` next to the Mouser program and run Mouser from there. If that folder exists, Mouser is portable:

- config.toml, state.json, logs and secrets all live inside `MouserData/` (secrets as 0600 files in `MouserData/secrets/`). Nothing is written to the computer's own config folder.
- The window's web data goes in `MouserData/webview/` (on macOS this may be ignored by the system web view; unverified).
- Mouser never sets itself to start at login, and "Allow control at the login screen" is hidden.
- The control socket stays at its normal path, so `mouser` still finds it.
- The `mouser` command's offline `config` and `logs path|tail` do not know about `MouserData/`; they look in the normal folder. While the portable copy runs, they work normally.

Limitations shown in the app in portable mode: on macOS, permissions must be granted again on each computer you run Mouser from the drive on; on Wayland, the desktop keeps Mouser's permission after you remove the drive; on Windows, portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del or applications running as administrator. The release workflow does not yet package a portable zip, so you create `MouserData/` yourself. On macOS the program sits inside `Mouser.app/Contents/MacOS/`, which is where the folder has to be; this is not verified on a real Mac.

## The login-screen (headless) copy

`mouser-app --headless` runs the engine with no window and no tray. It is what the macOS login-window agent starts. It does not use your user's config folder; it reads a machine folder, and keeps its secrets in `secrets/` inside it:

| OS | Machine folder |
|---|---|
| macOS | `/Library/Application Support/Mouser` |
| Windows | `C:\ProgramData\Mouser` |
| Linux | `/var/lib/mouser` |

The app's "Allow control at the login screen" (macOS only) copies your `config.toml`, desk keys and `state.json` there after one administrator prompt. The copy is a snapshot: it is not refreshed when you join, leave or rotate a desk's key, so run the option again after any of those. The macOS login-window mode has not been run on a real login screen. Stop a headless Mouser with SIGTERM or Ctrl-C.

## Live reload

Mouser looks at `config.toml` about every 250 ms while it runs. When the text changes it checks the whole file; if it is valid, the new values apply at once, exactly as if you had changed them in the app or with `mouser config set`. No restart.

**A bad edit is refused whole.** If any part of the file is wrong (not valid TOML, a missing key, a value out of range, an unparseable hotkey), Mouser ignores the entire edit, keeps every previous value, and (if logging is on) writes one `config.rejected` warning to the log with the reason in `fields.reason`. It does not show a notice in the app. Fix the file and save again; the corrected file is picked up.

Rules for the file:

- Every key must be present. A missing key makes the file invalid (there are no per-key defaults when reading). The simplest way to be safe is to edit values only, not delete lines, or to delete the whole file and let Mouser write a fresh one with the defaults.
- Unknown keys are ignored.
- Mouser rewrites the file (in its own order, without your comments) when it saves a change made through the app or the CLI.
- Desk-shared keys (`crossing.block_fullscreen`, `crossing.equalize_dimensions`, `advanced.allowlist`) go to the whole desk with a new version number when they change here, so the newest edit on any computer wins.
- Reload only watches the file while Mouser is running. At start, an invalid `config.toml` stops Mouser with an error instead of being refused, so keep a copy before big edits.
- `advanced.port` is read when Mouser starts. Changing it needs a restart.

## The keys and their defaults

This is the complete default file.

```toml
schema = 1

[general]
start_at_login = true
notifications = true
computer_name = ""

[hotkeys]
enable_disable = "Ctrl+Alt+Shift+M"
quickjump = "Ctrl+Alt+Shift+J"
targeted_paste = "Ctrl+Alt+Shift+V"
panic = "Escape*2"

[mouse]
buttons_4_5 = "passthrough"

[crossing]
block_fullscreen = true
equalize_dimensions = false

[diagnostics]
logging = false
max_log_mb = 50

[updates]
automatic = true
release_url = "https://api.github.com/repos/stefanrusek/mouser/releases?per_page=10"

[advanced]
port = 47801
allowlist = []
```

### `schema`

Always `1`. Read-only; `mouser config set schema ...` is refused.

### `[general]`

| Key | Default | Valid values and effect |
|---|---|---|
| `start_at_login` | `true` | true or false. Whether Mouser starts when you sign in. Applied once this computer is on a desk. Ignored in portable mode. |
| `notifications` | `true` | true or false. When false, Mouser shows no system notifications (problem notifications, the offer to check where a computer's screens sit, a failed Ctrl-Alt-Del from the tray). Windows such as the join prompt and the arranger still open. Changes apply live, no restart. |
| `computer_name` | `""` | Any text. Empty means the operating system's computer name. Other computers on the desk see a new name after they next reconnect. |

### `[hotkeys]`

Hotkeys are written as modifiers joined with `+`, then a key: `Ctrl+Alt+Shift+M`. Modifiers are `Ctrl`, `Alt`, `Shift` and `Meta` (`Cmd` also works). Key names are `A` to `Z`, `0` to `9`, `F1` to `F12`, `Escape`, `Space`, `Enter`, `Tab` and `Backspace`. The modifiers held must match exactly.

| Key | Default | Meaning |
|---|---|---|
| `enable_disable` | `Ctrl+Alt+Shift+M` | Pause or resume crossing **on this computer**. Registered with the operating system by the app. Mouser does not check this value when you set it; an accelerator the system cannot register is unverified territory, so keep to the format above. |
| `quickjump` | `Ctrl+Alt+Shift+J` | Shows a letter on every display; press a letter to jump there. Must parse as a hotkey; otherwise the change is refused. |
| `targeted_paste` | `Ctrl+Alt+Shift+V` | Pastes the latest clipboard content from another computer, fetching all of it. Must parse as a hotkey. |
| `panic` | `Escape*2` | `<key>*<count>`: that key pressed `count` times (1 or more), each press within 500 ms of the last, while the pointer is on another computer. It brings the pointer back. Written with `*`, not `+`. |

### `[mouse]`

| Key | Default | Valid values |
|---|---|---|
| `buttons_4_5` | `"passthrough"` | `"passthrough"` sends buttons 4 and 5 as they are. `"back_forward"` turns them into Back and Forward (Alt-Left and Alt-Right on Windows and Linux, Cmd-[ and Cmd-] on macOS), on press. |

### `[crossing]` (shared by the whole desk)

| Key | Default | Meaning |
|---|---|---|
| `block_fullscreen` | `true` | Do not cross off a computer while a full-screen app is in front. Works on macOS and Linux X11. On Wayland Mouser cannot tell, so crossing is never blocked there. |
| `equalize_dimensions` | `false` | Map one edge's whole length onto the edge next to it, so two displays of different sizes line up. The arranger sets this too. |

Corner blocking is not a config key. It is a per-edge option in the display arranger (default on, 8 logical pixel corners; push an edge twice within 600 ms to cross anyway).

### `[diagnostics]`

| Key | Default | Meaning |
|---|---|---|
| `logging` | `false` | Write the diagnostic log. Same as `mouser logs enable` and `disable` and the checkbox in the health window. |
| `max_log_mb` | `50` | A whole number. When `mouser.log` would grow past this many megabytes, it is renamed to `mouser.log.1` (replacing the older one) and a new file starts. Use 1 or more. |

### `[updates]`

| Key | Default | Meaning |
|---|---|---|
| `automatic` | `true` | One desk computer checks for new releases (about 30 seconds after start, then every 6 hours), downloads them and shares them with the rest of the desk, which install them. Turn off to update only with `mouser update`. |
| `release_url` | the GitHub releases list shown above | Where to look. It may point at a `latest.json`, a GitHub release object, or a GitHub releases array (newest first; drafts and releases without `latest.json` are skipped; pre-releases are included). It is not validated when set. Older 0.1.x files that still hold the unusable placeholder `https://api.github.com/repos/<owner>/mouser/releases/latest` are rewritten to the default at start. |

This check is the only internet access Mouser makes.

### `[advanced]`

| Key | Default | Meaning |
|---|---|---|
| `port` | `47801` | The UDP (QUIC) port. 1 to 65535. Needs a restart. Open it on firewalls between computers. |
| `allowlist` | `[]` | A list of networks in CIDR form, for example `["192.168.1.0/24", "10.0.0.0/8"]`. Empty means off (anyone on the network may ask to join, subject to approval). When set, a computer whose address is outside every listed network is refused when it asks to join, with a "refused because its address is not on the allowlist" notice. Shared by the whole desk. |

## What each rejection says

`mouser config set` (and the app) refuse a bad value with `key: reason` and keep the old value:

| Message | Cause |
|---|---|
| `mouse.buttons_4_5: must be "passthrough" or "back_forward"` | Any other text. |
| `advanced.port: must be between 1 and 65535` | Out of range. |
| `advanced.allowlist: "<entry>" is not a CIDR network` | An entry is not like `10.0.0.0/8`. |
| `hotkeys.panic: ...` | Not `<key>*<count>`, unknown key, or count below 1. |
| `hotkeys.targeted_paste: ...` and `hotkeys.quickjump: ...` | Unknown key or modifier. |
| `schema: read-only` | You tried to change `schema`. |
| `<section>: is a section; set its keys` | You tried to set `crossing` and the like. |
| `<key>: unknown key` | No such setting. |
| `<key>: expected true or false` / `expected a number` | Wrong kind of value. |

Known gap: when Mouser is **not** running, `mouser config set hotkeys.quickjump <value>` does not check the value (the running app does). A bad value written that way makes the next start fail, so check it yourself or set it while Mouser is running.
