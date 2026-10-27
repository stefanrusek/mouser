# Contract: diagnostic log events (v2)

**Record shape.** The record shape is unchanged from `specs/001-mouser-v1/contracts/log-record.schema.json`: `ts`, `level`, `computer{id,name}`, `event`, `peer?` and `fields?`. What is new in v2 is that `fields` is **closed per event**. `log-events.schema.json` lists every event and its allowed fields, and an event or field missing from it is a contract violation. Core builds records only through `mouser_core::logging::events::*` constructors, one per row below. Platform crates write through their diagnostics sink (Windows `diag::note`) with names from this catalog.

## Privacy rules (FR-003)

These rules are enforced by the closed schema and by test `contract_log_privacy` (SC-004).

- Never logged: typed characters, key usages or key names, clipboard contents, file contents, file names or paths of transferred files, window titles.
- Allowed: application names (the executable's name without its path), computer names, display names, desk names, setting keys and values, counts and sizes. The only paths allowed are Mouser's own log file and config file paths, and only in `log.header` and `logging.enabled`.

## Catalog

The **Spec** column cites spec.md (Story n sc. m). A `peer` column value of ✓ means the record's `peer` field is set.

| event | level | peer | fields | Spec |
|---|---|---|---|---|
| `log.header` | info | – | `mouser_version`, `os`, `os_version`, `computer_id`, `computer_name`, `desk_id?`, `desk_name?`, `mode` (installed/portable/portable_elevated/service/headless), `log_path` | S2 sc.1 |
| `logging.enabled` / `logging.disabled` | info | – | `path` (as 001) | 001 |
| `control.changed` | info | – | `from` (computer id), `to` (computer id), `how` (crossing/panic/quickjump/pause/link_lost/capture_lost/elevated_app/fault/quit/entered), `app?`, `app_admin?` (bool). `app`/`app_admin` are present when this computer is `to`. | S2 sc.2 |
| `pointer.hidden` | info | – | `reason` (crossing_out/quickjump) | S2 sc.3 |
| `pointer.shown` | info | – | `reason` (returned/capture_lost/fault/quit/startup_recovery/guard) | S2 sc.3 |
| `input.refused` | warn | ✓ (the controller) | `kind` (key/button/motion/scroll/grab/warp), `app?`, `app_admin?`. Written for the first refusal of a kind after an accepted input of that kind or after control changed. | S2 sc.4, S1 sc.1 |
| `input.refused_run_ended` | info | ✓ | `kind`, `count` (refusals in the run) | S2 sc.4 |
| `peer.connected` | info | ✓ | `version`, `os` (fields added) | 001 |
| `peer.disconnected` | info | ✓ | `reason` (peer_left/peer_quit/removed/replaced/unreachable/incompatible/protocol/fault_here/quit_here) | S2 sc.5 |
| `wire.unknown` | warn | ✓ | `variant` (index) | V3 |
| `fault` | error | – | `where` (thread or task name), `location` (`file:line`), `message` (panic payload when it is a static string or a format of Mouser's own text; otherwise `"non-string payload"`) | S2 sc.6 |
| `fault.recovered` | warn | – | `where`, `action` (returned_local/closed_link/continued) | S2 sc.6 |
| `display.changed` | info | – | `computer`, `display` (name), `change` (added/removed/rearranged/resized) | S2 sc.7 |
| `layout.changed` | info | – | `by` (computer id), `computer` (moved computer), `change` (moved/display_removed/display_included/kept_placement/auto_placed) | S2 sc.7 |
| `setting.changed` | info | – | `key`, `old`, `new`, `origin` (app/cli/file/desk_mate), `from?` (desk-mate id) | S2 sc.8 |
| `clipboard.sent` / `clipboard.received` | info | ✓ | `from`, `to`, `reps`: [{`kind` (text/html/rtf/png/files), `size` (bytes, or item count for files)}] | S2 sc.9 |
| `logs.collected` | info | – | `path`, `included` (count), `missing`: [{`computer`, `reason`}] | S2 sc.10 |
| `elevation.restart` | info | – | `outcome` (prompted/declined/handed_over) | S5 |
| `problem.raised` / `problem.cleared` | as 001 | – | `kind`, `problem_id`, `computer` (added), `message` | 001 |
| `config.rejected`, `update.*` | as 001 | | unchanged | 001 |
| `capture.probe`, `capture.reinstall`, `capture.hook_thread`, `capture.start` | debug | – | as 001 (2026-10-23 decision), plus `capture.probe.outcome` value `live` (real input seen, V7) | S3 sc.16 |
| `capture.lost` | warn | – | `reason` (elevated_foreground/other_desktop/hooks_silent), `foreground?` (program name) | S1 sc.3 |
| `capture.desktop` | info | – | `desktop` (default/winlogon/other) (session engine, V9) | S5 |
| `service.*` (service log only) | info | – | `service.session` {`session`}, `service.engine_started` {`pid`, `session`}, `service.engine_exited` {`code`}, `service.sas` {`result`} | S5 |

## Rotation and header

- `mouser.log` is rotated to `mouser.log.1` when the next record would make it exceed `max_log_mb/2` MiB, and the previous `mouser.log.1` is deleted.
- Every new file starts with `log.header` (FR-008, S2 sc.1).
- The service writes its own `service.log` in the machine folder under the same rules.
