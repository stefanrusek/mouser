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
| `logs.collect` | `{dir?: string}` | `{path, computers: [{id, name, included, reason?}]}` (log-bundle.md) | -32603 when the bundle cannot be written (the OS reason is in `message`) |
| `desk.rename` | `{computer: id, name: string}` (1–64 chars, trimmed) | `{}` | -32602 if the name is invalid; -32001 `data.kind=join_refused` if this computer is not on a desk |
| `layout.keep_placement` | `{computer: id}` | `{}` | clears `needs_review` for that computer's displays and logs `layout.changed change=kept_placement` |
| `elevation.restart` | `{computer: id}` | `{}` | for this computer: emits `EngineEvent::RestartElevated`. For a peer: sends wire `RestartElevated`. -32001 `data.kind=platform_limitation` when the target is not portable Windows, or is a peer older than 0.3.0 |

## Methods: changed

- **`config.set`** gains an optional `origin: "app" | "cli"` (default `"cli"`). It is recorded in `setting.changed`. Setting `general.computer_name` also renames this computer desk-wide (same effect as `desk.rename`).
- **`status`** returns `status.schema.json` v2 (in this directory), which adds `tray_summary`, `computers[].mode`, `computers[].os_version`, `problems[].problem_id`, `problems[].app` and the new fix values.

## Topics

- **`config` (new).** Each notification is `{key, old, new, origin, from?}`, sent once per changed key from every source: app, cli, file or desk_mate. The app re-registers OS hotkeys when it gets one.
- **`problem` (changed).** The notification adds `problem_id` and `computer` (already present), plus `app`. Consumers key problems by `(problem_id, computer)`.
- **`status` (changed).** It carries the v2 status.

## Engine events (app ↔ core, in-process, `mouser_core::EngineEvent`)

The engine adds two events:

- `RestartElevated`: run the V19 sequence.
- `Fault{where}`: the app logs nothing extra. This event exists only so tests can observe that recovery happened.

The existing `Landing` event is no longer sent when `crossing.landing_highlight` is false.
