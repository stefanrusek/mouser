# Contract delta: `mouser` CLI and app/service command lines (v2)

This file lists changes to `specs/001-mouser-v1/contracts/cli.md`. The exit codes are unchanged: 0 means success, 1 means error, 2 means Mouser is not running, 3 means an invalid argument.

## `mouser` CLI (apps/mouser-cli)

| Command | Behaviour | Spec |
|---|---|---|
| `mouser logs collect [--out DIR] [--json]` (new) | Calls `logs.collect` (with `dir` = `--out`); when `status` reports this computer's mode as `service` (Windows installed) it calls `logs.collect_stream` instead and writes the zip itself, as the calling user, into `--out` or the user's Downloads folder (owner decision OD-7). Text output is the bundle path, then one line per computer: `<name>: included` or `<name>: not included (<offline\|logging off\|older version>)`. `--json` prints the method's result. If the engine is not running it exits 2. | Story 2 sc. 10 |
| `mouser desk rename <computer> <name>` (new) | Calls `desk.rename`. `<computer>` is a name or an id. | Story 4 sc. 3 |
| `mouser config set …` (changed) | Sends `origin: "cli"`. Offline `config set hotkeys.enable_disable` is now validated (001 docs said it was not). | Story 2 sc. 8 |
| `mouser status` (changed) | The text output shows each computer's friendly name, version and mode, and the tray connection state. JSON follows status.schema.json v2. | Story 3 sc. 4, 8 |

**Endpoint selection:** on Windows the CLI connects to `\\.\pipe\mouser-service` when it exists (installed), and otherwise to the per-user pipe. The CLI uses the same `default_path()` as core.

## `mouser-app` command line (apps/mouser-app)

| Flag | Behaviour |
|---|---|
| `--headless` | Runs the engine with no window or tray, until it gets SIGTERM, Ctrl-C or a service stop. **macOS:** the machine folder, unchanged from 001. **Windows and Linux** (new, which supersedes T516's refusal): the user's folder, or `MouserData/` when portable. |
| `--headless --machine` (new) | Uses the machine folder (`C:\ProgramData\Mouser`, `/var/lib/mouser`, `/Library/Application Support/Mouser`). This is how the Windows service starts the session engine. |
| `--cursor-guard <pid>` (new, Windows) | Runs the cursor guard (research V5). It has no window and exits when `<pid>` ends, after restoring the system cursors if they are blank. |
| `--wait-for <pid>` (new, Windows) | Waits for `<pid>` to exit before any Tauri initialisation (research V19). |
| `--install-login-screen <staging>` | Unchanged (macOS, 001). |

## `mouser-service` command line (apps/mouser-service-win, new)

| Invocation | Behaviour |
|---|---|
| `mouser-service.exe --install` | Needs an administrator. It creates `MouserService` (LocalSystem, automatic start, display name "Mouser"), sets `SoftwareSASGeneration=1`, adds the firewall rule, migrates the identity (research V12a) and starts the service. It exits 0, or 1 with the reason on stderr. |
| `mouser-service.exe --uninstall` | Stops and deletes the service, ends the session engine, and removes the firewall rule and the `SoftwareSASGeneration` value Mouser set. It exits 0 even if the service was absent. |
| started by the SCM, with no arguments | Runs the service (contracts/service-ipc.md). |
