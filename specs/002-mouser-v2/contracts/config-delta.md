# Contract delta: configuration (v2)

These are changes to `specs/001-mouser-v1/contracts/config.md`. `schema` stays `1`: the changes only add keys, and a missing key takes its default (001 orchestrator decision of 2026-10-22).

> **Frozen-crate change.** This contract changes `crates/mouser-proto/src/config.rs` (`Config`, defaults, `validate`, `DESK_SHARED_KEYS`), which is frozen.

## New keys

```toml
[crossing]
landing_highlight = true      # desk-shared; Story 4 sc. 4

[keyboard]
command_as_control = true     # desk-shared; Story 4 sc. 5–6: Mac Command acts as Control on Windows/Linux
```

`DESK_SHARED_KEYS` adds `crossing.landing_highlight` and `keyboard.command_as_control`. Both travel in the wire message `DeskSettings`.

## Changed semantics

| Key | Change |
|---|---|
| `general.computer_name` | Setting it renames this computer **desk-wide**: it updates `Member.display_name` in `DeskState`. An empty value means the OS name. A rename made from another computer (`desk.rename`) writes the new name back into this computer's `general.computer_name` when it adopts the change. |
| `hotkeys.enable_disable` | Now validated by the same chord parser as `quickjump` and `targeted_paste`. 001 parsed it only at registration. |
| `hotkeys.panic` | Unchanged format `<Key>*<count>`, count 1–5 (001). Edited in Settings with a key recorder. |
| `diagnostics.max_log_mb` | Validated ≥ 1 (`diagnostics.max_log_mb: must be 1 or more`). The limit now caps the total of both log files (FR-008). |

## Machine folder (installed Windows, headless `--machine`)

`C:\ProgramData\Mouser\config.toml` holds the session engine's settings. The UI app and the CLI edit them through the control API (`config.set`), never through the file. Direct file edits by an administrator still apply through the file watch.

## Validation messages (added)

- `crossing.landing_highlight: must be true or false`
- `keyboard.command_as_control: must be true or false`
- `hotkeys.enable_disable: <chord parse reason>`
