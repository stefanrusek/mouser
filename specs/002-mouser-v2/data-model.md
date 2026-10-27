# Data Model: Mouser v2 (differential)

This document lists only what changes from `specs/001-mouser-v1/data-model.md`. Anything not mentioned here is unchanged. Wire shapes are in `contracts/wire-protocol-delta.md`, and the decisions referred to (V1–V20) are in `research.md`.

## ActiveSession (amended): control moves and recovery

The 001 transitions (`Local`, `Remote(c)`, receiving) stay as they were. v2 adds the following triggers, and each one now names the `how` that is written to the `control.changed` log record (contracts/log-events.md).

| Trigger | From → To | how | Effects |
|---|---|---|---|
| edge crossing | Local → Remote(c), Remote(c) → Remote(d), Remote(c) → Local | `crossing` | as in 001 |
| QuickJump | any → Remote(c) / Local | `quickjump` | as in 001 |
| panic key (local) or `Panic` received | Remote(_) → Local | `panic` | as in 001 |
| pause (tray, left click, hotkey, CLI) | Remote(_) → Local | `pause` | as in 001 |
| link lost | Remote(c) → Local | `link_lost` | 001 Story 3 sc. 5. The record carries the link-loss `reason`. |
| **capture lost** (`LocalInput::CaptureLost`) | Remote(c) → Local | `capture_lost` | The backend has already ungrabbed and shown the pointer (V7). Core sends `ReleaseAll` to c. |
| **receiver: foreground outranks Mouser** | (receiving from k) → not receiving; at k: Remote(this) → Local | `elevated_app` | The receiver releases what it holds, sends `Panic` to k only, and raises `elevated_app` on itself (V6). |
| **fault recovered** | any → Local | `fault` | `Crossing::recover_after_fault()`: ungrab, show the pointer, release what is held, and send `ReleaseAll`/drop the receiving session (V3). |
| **quit / session end** | any → Local | `quit` | `Engine::shutdown` (V4). Links close with reason `quit`. |
| Enter from another computer | Remote(c) → Local (driven) | `entered` | as in 001 |

**Invariants added by v2:**

- The pointer is hidden on a computer only while it is in state `Remote(_)` and its capture is grabbed. Every transition to `Local` shows the pointer first, then writes `pointer.shown`.
- No input result (Ok or Err) can change the state, except the explicit `capture_lost`. A refused inject never leaves the state machine half-updated (V1).
- After any `catch_unwind` fault in a unit of work, the crossing state is `Local` and nothing is held (V3).

## Link loss reason (new)

`LinkLossReason` ∈ {`peer_left` (the peer closed with `done` or left the desk), `peer_quit` (the peer closed with `quit`/`shutdown`), `removed` (`not a member`), `replaced` (superseded by a newer link to the same peer), `unreachable` (idle timeout or path lost), `incompatible` (protocol mismatch), `protocol` (an undecodable frame), `fault_here` (this computer's link task faulted), `quit_here` (this computer is shutting down)}.

- **Source of the reason:** quinn `ConnectionError` and `close_reason()`, together with the application close reason bytes.
- **Where it is used:** the reason feeds the `peer.disconnected` record and decides whether the tray treats the disconnection as unexpected.
- **Close reasons sent:** `quit` (replacing `shutdown`), `fault`, `protocol`, `incompatible`, and the existing `done`, `replaced` and `not a member of this desk`. A peer reports the bytes `shutdown` it receives from a 0.2.x computer as `peer_quit`.

## Connection state for the tray (new)

`TrayConnection` ∈ {`connected`, `alone`, `disconnected`}, computed on every status rebuild:

```text
no desk, or desk with no other member                 → alone
every non-removed member is connected (paused counts) → connected
≥1 non-removed member is reconnecting/offline/incompatible → disconnected
```

- **What it does not depend on:** whether this computer is paused. The icon shows paused and the connection state together (spec edge case).
- **Removed members:** a member that the owner removed, or this computer leaving the desk, takes that member out of `members`, so it never counts.
- **Where it is published:** `tray_summary.connection` (status.schema.json).

## Problem (amended)

| Change | Rule |
|---|---|
| Attribution key | Problems are unique per `(problem_id, computer)` everywhere: the core merge, the UI and toasts. |
| Start-up limitations | Startup limitations are no longer raised as problems. The ids `platform_limitation:<capability>` are never raised in v2, and when they arrive from 0.2.x peers they are dropped on merge. Limitations live only in `Computer.limitations`. |
| New kind `elevated_app` | Fields: `computer` = the Windows computer the app runs on, `message` = "Mouser on <computer> can't control <app> because it runs as administrator.", `app` (new optional field) = the program name. The fix is `restart_elevated:<computer id>` when that computer is portable and not elevated; with no fix the message also names installing Mouser (Story 5 sc. 6). It is raised by the receiver (V6) and cleared when control next enters that computer without an outranking foreground. It toasts on the controller once per raise (`QUIET_FOR` as in 001). |
| New fix actions | `restart_elevated:<computer id>`, `keep_placement:<computer id>`, `update_computer:<computer id>`. The last one is offered when the incompatible computer is this one and an update is available locally (Story 3 sc. 8). |
| LOST_HOLD | It is raised only after real-input misses (V7). The Story 3 sc. 16 case therefore raises nothing. |

## Limitation (amended)

- `Limitation { capability, text }` is unchanged in shape.
- **Text rule:** every text describes the computer it is listed under ("on this computer"). Where it renders: only in that computer's Health entry, under "What doesn't work here".
- **New texts:**

| Where | Text | Capability |
|---|---|---|
| Windows portable, not elevated | "Portable Mouser can't control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator." | SecureAttention |
| Windows portable, elevated | "Portable Mouser can't control the login screen, permission prompts or Ctrl-Alt-Del." | SecureAttention |
| Windows, policy blocks software SAS | "Ctrl-Alt-Del is blocked by this PC's policy." | SecureAttention |
| Windows, every build | "Windows warns that Mouser comes from an unknown publisher when it is installed or first run." | Install (new Capability variant) |
| Linux | "Mouser can't control this computer's login screen." | LoginScreen (new Capability variant) |
| Linux, tray without click events | "Clicking the tray icon doesn't pause Mouser here; use Pause in its menu." | Tray (new Capability variant) |
| Linux, no tray | Same text as 001. It moves from `Capture` to `Tray`. | Tray |

## Computer (amended)

| Field | Change |
|---|---|
| `display_name` | Now the **desk-wide friendly name**: `Member.display_name` in `DeskState`, settable from any desk computer (`desk.rename`). It defaults to the OS name at join. A change to `general.computer_name` renames this computer desk-wide. |
| `version` | Already present. Desk and Health show it for every computer (Story 3 sc. 8). |
| `mode` (new, status) | `installed` / `portable` / `portable_elevated` / `service` / `headless`. It selects limitation texts and fixes. Peers report it in `PeerStatus` (≥ 0.3.0). For 0.2.x peers it is unknown (absent). |
| `has_pointer` (derived) | Comes from the top-level `active_computer`, which the 001 schema already defines and which core now always fills in (contracts/status.schema.json). Desk shows which computer has the pointer. |
| `problem_count` (derived, UI) | The number of open problems with this `computer`. It links to Health#<id>. |

## Desk settings (amended): desk-shared

| Key | Default | Carried by |
|---|---|---|
| `crossing.block_fullscreen`, `crossing.equalize_dimensions`, `advanced.allowlist` | as 001 | `DeskState` (unchanged) |
| **`crossing.landing_highlight`** | true | **`DeskSettings`** (new, ≥ 0.3.0) |
| **`keyboard.command_as_control`** | true | **`DeskSettings`** |
| friendly names | OS name | `DeskState.members[].display_name` |

- **Versioning.** `DeskSettings { epoch: u64, author: ComputerId, landing_highlight, command_as_control }` is versioned on its own. The higher `(epoch, author)` wins, as for DeskState.
- **Older peers.** Peers on 0.2.x never receive it. They behave as the defaults: highlight on, Command as Control.

Per computer, unchanged keys: `hotkeys.panic`, `hotkeys.quickjump`, `hotkeys.enable_disable`, `hotkeys.targeted_paste`, `general.*`, `mouse.buttons_4_5`, `diagnostics.*`.

## Setting change (new)

`SettingChange { key, old, new, origin }`, where `origin` ∈ {`app`, `cli`, `file`, `desk_mate:<computer id>`}.

- **When it is emitted:** for each key that differs after an apply.
- **Where it goes:** to the `config` control topic (the app re-registers hotkeys from it) and to `setting.changed` in the log.
- **How each origin is known:**

| Origin | How it is known |
|---|---|
| `app`, `cli` | the `origin` parameter of `config.set` |
| `file` | a file-watch reload |
| `desk_mate` | adopting a winning `DeskState` or `DeskSettings` |

## Log record (amended)

- **Shape.** The 001 `LogRecord` shape is unchanged (`ts`, `level`, `computer`, `event`, `peer?`, `fields?`).
- **Fields.** `fields` is now constrained per event by `contracts/log-events.schema.json`, a closed catalog.
- **Where records come from.** Core builds records only through typed constructors (V14).

**Log file lifecycle:**

```text
open/rotate ──▶ write `log.header` ──▶ append records
     ▲                                       │
     └── size(mouser.log)+len > max/2 ───────┘   (mouser.log → mouser.log.1, old .1 deleted)
```

**Validation:**

- `diagnostics.max_log_mb` ≥ 1 (enforced by `Config::validate`).
- The two files together are always ≤ `max_log_mb` MiB.

## Log bundle (new)

| Field | Rules |
|---|---|
| file | `mouser-logs-<desk name, sanitised>-<UTC yyyymmddThhmmssZ>.zip`, written on the requesting computer only (FR-007) |
| `bundle.json` | `contracts/log-bundle.schema.json`: `created_at`, `requested_by` (computer id), `desk {id, name}`, `computers[]` |
| computers[] | `id`, `name`, `os`, `os_version`, `version`, `mode`, `connection` (as seen by the requester), `included: bool`, `reason?` ∈ {`offline`, `logging_off`, `older_version`}, `files[]` (relative paths), `settings_file?` |
| per computer folder | `<name>-<first 8 hex of id>/`: `config.toml` (secrets never live there, and the file is copied verbatim), `mouser.log`, `mouser.log.1` (present only if it exists) |

**Never included:** `state.json`, `secrets/`, the desk key, the device key.

**Collection states** (one per computer):

```text
requested ─stream complete─▶ included
    │──link lost / not connected─▶ offline
    │──peer < 0.3.0────────────▶ older_version (never requested)
    └──peer has logging off────▶ included=false, reason logging_off (card + settings still included)
```

## Elevation (new, Windows)

`ElevationMode` per Windows computer:

```text
installed (session engine as SYSTEM) — never outranked except on desktops V9 follows
portable ─Restart as administrator (UAC approved)─▶ portable_elevated
portable ─UAC declined─▶ portable (problem stays)
```

**Restart sequence states:** `idle → prompting → handing_over (old instance shutting down) → elevated_running`. A declined UAC prompt returns to `idle`.

## Service and session engine (new, Windows installed)

| Entity | Fields | States |
|---|---|---|
| ServiceState | `console_session: Option<u32>`, `engine: Option<EnginePid>`, `sas_policy: 0..3` | `Starting → WaitingForSession (session id 0xFFFFFFFF) → Running(session, pid)`. On a console session change: `Running → Relaunching → Running`. If the engine exits: `Running → Relaunching`. On stop: `Stopping` (it sends `Shutdown` and ends the engine). |
| SessionEngine | `session_id`, `desktop: Default\|Winlogon\|Other(name)` | It follows the input desktop. Each switch reinstalls the hooks on the new desktop and re-attaches injection. |

## UI navigation state (new)

`Route` ∈ {`desk`, `displays`, `health`, `health/<computer id>`, `settings`, `settings/<setting key>`, `diagnostics`, `help`, `welcome` (first run)}.

- The window opens on `desk`, or on `welcome` while `needs_setup` is true.
- **Search index:** a static list of `{route, title, keywords}` covering every section and every setting (contracts/ui-navigation.md). SC-005 (≤ 2 clicks) is checked against it.
