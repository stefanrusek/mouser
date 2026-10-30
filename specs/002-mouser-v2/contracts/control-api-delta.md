# Contract delta: control API (v2)

These are changes to `specs/001-mouser-v1/contracts/control-api.md`: newline-delimited JSON-RPC 2.0 over the local socket or pipe.

> **Frozen-crate change.** This contract changes `crates/mouser-proto/src/control.rs` and `schemas/status.schema.json`. It needs approval as part of the 002 Foundational phase.

## Transport

| Mode | Endpoint | Access |
|---|---|---|
| macOS, Linux, Windows portable or per-user | unchanged (001; the Windows pipe stays `\\.\pipe\mouser-<USERNAME>`, and the contract text is corrected to say so) | the user |
| **Windows installed** (session engine, research V9) | `\\.\pipe\mouser-service` | DACL: SYSTEM, Administrators, and the interactive user of the console session. `PIPE_REJECT_REMOTE_CLIENTS`. |

`mouser_core::control::default_path()` returns the service pipe when it exists, otherwise the per-user path.

## Methods: new

| Method | Params | Result | Errors |
|---|---|---|---|
| `logs.collect` | `{dir?: string}` | `{path, computers: [{id, name, included, reason?}]}` (log-bundle.md) | -32603 when the bundle cannot be written (the OS reason is in `message`). **Mode `service`** (Windows installed session engine): the engine writes no file; a call with `dir` returns -32602 `"logs.collect: the installed Mouser service does not write to a chosen folder; use logs.collect_stream"`, and a call without `dir` returns the same error (owner decision OD-7) |
| `logs.collect_stream` (new, every mode; required in mode `service`) | `{}` | `{name, size, sha256, computers: [{id, name, included, reason?}]}`, sent after the last `logs.chunk` notification | -32603 when the bundle cannot be built. The engine gathers the parts as for `logs.collect`, builds the zip in memory and sends it to the calling connection only as notifications `logs.chunk {seq, data}` (`seq` from 0, `data` base64 of at most 65536 bytes). The caller writes `name` into the user's Downloads folder or the folder the user chose, as the user, applying log-bundle.md's naming and collision rules, and checks `size` and `sha256` |
| `desk.rename` | `{computer: id, name: string}` (1–64 chars, trimmed) | `{}` | -32602 if the name is invalid; -32001 `data.kind=join_refused` if this computer is not on a desk |
| `layout.keep_placement` | `{computer: id}` | `{}` | clears `needs_review` for that computer's displays and logs `layout.changed change=kept_placement` |
| `elevation.restart` | `{computer: id}` | `{}` | for this computer: emits `EngineEvent::RestartElevated`. For a peer: sends wire `RestartElevated`. -32001 `data.kind=platform_limitation` when the target is not portable Windows, or is a peer older than 0.3.0 |
| `problem.fix` (T530) | `{problem_id: string, computer: id}` | `{}` | Carries out the fix of the open problem `(problem_id, computer)`, as the in-process `ControlHandle::fix` does: opens the system screen for `open_system_settings:*`, clears an `open_arranger_edge` notice, then announces the new `status`. -32602 when that pair is not an open problem, or when its fix is the app's own (any other fix value) |
| `transfer.resolve` (T530) | `{transfer_id: string, overwrite: bool}` | `{}` | Answers a `transfer` notification in state `awaiting_overwrite_decision` (the in-process `ControlHandle::resolve_transfer`). An unknown `transfer_id`, or a platform that cannot drag files, is ignored |

## Methods: changed

- **`config.set`** gains an optional `origin: "app" | "cli"` (default `"cli"`). It is recorded in `setting.changed`. Setting `general.computer_name` also renames this computer desk-wide (same effect as `desk.rename`).
- **`status`** returns `status.schema.json` v2 (in this directory), which adds `tray_summary`, `computers[].mode`, `computers[].os_version`, `problems[].problem_id`, `problems[].app` and the new fix values.

**Who calls which (OD-7):** the CLI and the app read `status` first; when this computer's `mode` is `service` (the endpoint is the Windows installed session engine) they call `logs.collect_stream` and write the file themselves as the user, otherwise they call `logs.collect`. The service-mode engine therefore never writes a file to a path a user chose.

## Topics

- **`config` (new).** Each notification is `{key, old, new, origin, from?}`, sent once per changed key from every source: app, cli, file or desk_mate. The app re-registers OS hotkeys when it gets one.
- **`problem` (changed).** The notification adds `problem_id` and `computer` (already present), plus `app`. Consumers key problems by `(problem_id, computer)`.
- **`status` (changed).** It carries the v2 status.
- **`engine_event` (new, T530).** Each notification's params is one UI-facing engine event, tagged by `event`:
  - `{"event": "landing", "display": {computer, key}, "x": f32, "y": f32, "desktop": [f64, f64]}`
  - `{"event": "review_offered", "computer": id, "name": string}`
  - `{"event": "quick_jump_show", "letters": [[{computer, key}, "A"], ...]}`
  - `{"event": "quick_jump_hide"}`
  - `{"event": "restart_elevated"}`

  These mirror the in-process engine events below, for the app in UI mode (Windows installed), which reaches the session engine only over this API. `Fault` is not sent.

## Engine events (app ↔ core, in-process, `mouser_core::EngineEvent`)

The engine adds two events:

- `RestartElevated`: run the V19 sequence.
- `Fault{where}`: the app logs nothing extra. This event exists only so tests can observe that recovery happened.

The existing `Landing` event is no longer sent when `crossing.landing_highlight` is false.
