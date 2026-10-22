# The `mouser` command

`mouser` is the command-line client of a running Mouser. It is for scripts, troubleshooting agents and anyone who prefers a terminal. Everything it does is also available in the app.

It talks to the Mouser app on **this** computer through a local socket, so it only controls and reports on the computer you run it on. The few commands that work without the app running are marked below.

Where to find the program:

- macOS: `/Applications/Mouser.app/Contents/MacOS/mouser`. To put it on your PATH: `sudo ln -s /Applications/Mouser.app/Contents/MacOS/mouser /usr/local/bin/mouser`.
- Linux: the `.deb` installs `/usr/bin/mouser`. From the AppImage, extract it (`--appimage-extract`) and use `usr/bin/mouser`.
- Windows: `mouser.exe` next to `Mouser.exe` in `%LOCALAPPDATA%\Mouser`. There is no Windows backend in v1, so Mouser itself does not run there.

## Global options

| Option | Meaning |
|---|---|
| `--json` | Print machine-readable JSON instead of text. Works on every command and may come before or after the command name. |
| `-h`, `--help` | Print help. |
| `-V`, `--version` | Print the version. |

`mouser --help`:

```
Control a running Mouser

Usage: mouser [OPTIONS] <COMMAND>

Commands:
  status    Desk, computers, connections, missing permissions and problems
  pause     Stop crossing on this computer only (run it on each computer to pause the desk)
  resume    Resume crossing on this computer only
  desks     Desks discovered on the network
  desk      Create, join or leave a desk, or remove a computer from it
  requests  Pending join requests
  approve   Allow a computer to join this computer's desk
  decline   Decline a computer's request to join this computer's desk
  config    Read or change settings (works on the file when Mouser is not running)
  update    Check for, or install, a Mouser update
  send-cad  Send Ctrl-Alt-Del to the Windows computer you are controlling
  logs      Diagnostic logging (path and tail work when Mouser is not running)
  help      Print this message or the help of the given subcommand(s)

Options:
      --json     Print machine-readable JSON instead of text
  -h, --help     Print help
  -V, --version  Print version
```

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success. `--help` and `--version` also exit 0. |
| 1 | Mouser answered with an error (for example "that computer is no longer on the desk you chose"), or the desk declined your join request. |
| 2 | Mouser is not running (nothing is listening on the control socket). The message is `Mouser is not running`. |
| 3 | An argument or value is not valid: an unknown command or option, a missing argument, a bad `config set` value (printed as `key: reason`), a desk name that matches nothing, or an engine "invalid params" error. |

Errors go to standard error as plain text, even with `--json`. Standard output carries only the result.

## Commands that need Mouser running

Without a running Mouser these exit 2: `status`, `pause`, `resume`, `desks`, `desk ...`, `requests`, `approve`, `decline`, `update`, `send-cad`, `logs enable`, `logs disable`.

### `mouser status`

Shows the desk, every computer and its connection, missing permissions and open problems.

```
Mac-mini: active
Desk: Home Desk [3737e2977ecd47aaa2958b69c55f46b0]
Computers:
  Mac-mini: this computer
  MacBook-Pro: connected, macos, 2 display(s)
Missing permissions: none
Problems:
  Mac-mini: Some games and remote-desktop apps ignore Mouser's input on this Mac.
```

The first line is `<this computer>: active` or `paused`. The desk line is `Desk: none`, or `Desk: <name> [<id>]`, with `(waiting for approval)` while a join is pending. A connection is one of `connected`, `reconnecting`, `offline`, `incompatible`. A problem with a suggested fix ends with `(fix: <action>)`.

`--json` prints the status object. It matches `specs/001-mouser-v1/contracts/status.schema.json` and adds a few fields per problem (`problem_id`, `raised_at`, `cleared_at`):

```
{"version", "computer":{"id","name"}, "state":"active|paused", "desk":null|{"id","name","pending_join"?},
 "active_computer", "computers":[{"id","name","os","version","connection","rtt_ms"?,"last_seen"?,
 "permissions":{kind:"granted|missing|not_applicable"},"limitations":[text],"displays"}],
 "problems":[{"problem_id","kind","computer","message","fix","raised_at","cleared_at"}],
 "logging":{"enabled","path"}}
```

`kind` is one of `missing_permission`, `incompatible_version`, `join_refused`, `secure_input`, `crossing_blocked`, `peer_offline`, `platform_limitation`. See [troubleshooting.md](troubleshooting.md).

### `mouser pause` and `mouser resume`

```
Stop crossing on this computer only (run it on each computer to pause the desk)
Resume crossing on this computer only
```

Prints `Paused` or `Resumed`; JSON is `{}`. **Pause is per computer.** The contract wanted it to apply to the whole desk; as built, run it on each computer. The default hotkey Ctrl+Alt+Shift+M does the same thing.

### `mouser desks`

Lists desks found on the network, one line each: `<name> [<desk id>]: <member names>`. Prints nothing if none were found.

JSON: `[{"desk_id": "...", "name": "...", "members": ["..."]}]`.

### `mouser desk create <NAME>`

Creates a desk with this computer as its only member.

Text: `Created desk "<name>" [<desk id>]`. JSON: `{"desk_id", "name", "state": {...}}` (the desk's shared state: epoch, members, layout, allowlist).

### `mouser desk join <DESK>`

Joins a desk and **waits for the decision** (no timeout). `<DESK>` is a desk name, a desk id, or `host:port` (a numeric port after the last colon means an address).

- A name that matches nothing: `no desk named "<name>" was found on the network`, exit 3.
- A name that matches several desks: lists their ids and asks you to join by id, exit 3.
- Approved: `Joined desk "<name>" [<desk id>]`, exit 0. JSON: `{"id", "name"}`.
- Declined, refused by the allowlist, incompatible version, or interrupted: the reason on standard error, exit 1.

Someone must approve on a computer already on the desk (`mouser approve`, or Allow in the app).

### `mouser desk leave`

Leaves the current desk. Text `Left the desk`; JSON `{}`.

### `mouser desk remove <COMPUTER>`

Removes a computer from the desk. `<COMPUTER>` is a computer id or a name as shown by `mouser status`. A name that is not on the desk exits 3. Text `Removed <computer> from the desk`; JSON `{}`.

### `mouser requests`

Lists pending join requests, one per line: `<request number>: <computer> wants to join <desk>`. Prints nothing if there are none.

JSON: `[{"request_id": 3, "computer": {"id","name"}, "desk": {"id","name"}}]`.

### `mouser approve <REQUEST>` and `mouser decline <REQUEST>`

`<REQUEST>` is the number from `mouser requests` (an unsigned integer). Text `Approved request N` or `Declined request N`; JSON `{}`. An unknown request number prints `there is no such join request` and exits 3.

### `mouser update [check|apply]`

```
mouser update            same as: mouser update check
mouser update check      Show the running version and the newest available one (the default)
mouser update apply      Install the available update on this computer and restart Mouser
```

Text for check:

```
Current version: 0.1.1
Available version: none
```

`apply` prints `Updating from <current> to <available>` and the app then restarts. JSON for both: `{"current": "0.1.1", "available": "0.2.0" | null}`. The update check is the only internet access Mouser makes (see `updates.release_url` in [configuration.md](configuration.md)).

### `mouser send-cad`

Sends Ctrl-Alt-Del to the Windows computer you are currently controlling. Text `Sent Ctrl-Alt-Del`; JSON `{}`. In v1 there is no Windows backend, so this exits 1 with `Ctrl-Alt-Del cannot be sent to <name>.`; it also does that when the computer you control is not Windows or input is not running.

### `mouser logs enable` and `mouser logs disable`

Turn diagnostic logging on or off (this is `diagnostics.logging` in config.toml, and it is saved). Text: `Logging is on: <path>` or `Logging is off`. JSON: `{"path": "<log file>"}`.

## Commands that also work when Mouser is not running

These read or write the files directly when the app is stopped, and go through the app when it is running.

### `mouser config get [KEY]`

`KEY` is `section.name` (for example `hotkeys.panic`), a whole `section` (for example `crossing`), or nothing for everything. A string prints bare; a number or true/false prints as is; a section or the whole config prints as indented JSON. `--json` prints the JSON value (a string is quoted). An unknown key prints `<key>: unknown key` and exits 3.

If there is no config.toml yet, it is created with the defaults. If config.toml is not valid, the offline command stops with an error message (it does not repair the file); see [troubleshooting.md](troubleshooting.md).

### `mouser config set <KEY> <VALUE>`

Changes one setting. The value is read as the same type as the setting's current value:

| Setting type | Write it as |
|---|---|
| true/false | `true` or `false` |
| number | a whole number, 0 or more |
| list (`advanced.allowlist`) | comma-separated, for example `10.0.0.0/8,192.168.0.0/16`; an empty string empties the list |
| text | the text itself |

An invalid value is refused with `key: reason` on standard error and exit 3, and the old value stays. `schema` is read-only; a section (like `crossing`) cannot be set, only its keys. With Mouser running the change applies immediately; with it stopped the file is rewritten. Text output is `key = value`; JSON is `{}`. Every key, default and valid value is in [configuration.md](configuration.md).

### `mouser config path`

Prints the full path of the installed config.toml (`{"path": "..."}` with `--json`). It always answers with the per-OS location, even for a portable copy of Mouser; see the last section.

### `mouser logs path`

Prints where the diagnostic log is (`{"path": "..."}` with `--json`), whether or not logging is on. Running: the path Mouser reports. Stopped: `<config dir>/logs/mouser.log`.

### `mouser logs tail [-n N]`

Prints the last N lines (default 20) of the log file as raw JSON Lines. It ignores `--json` because the output is already JSON. If there is no log file yet, it prints nothing. The log format is in [troubleshooting.md](troubleshooting.md).

## Known differences from the original contract

- `pause` and `resume` apply to this computer only.
- There is no `desk move` command, although the control API has a `desk.move` method.
- `config path`, and `config` and `logs path|tail` while Mouser is stopped, use the installed per-OS config directory. They do not look for a portable `MouserData/` folder. For a portable copy the file is `<folder with the app>/MouserData/config.toml`; `config get|set` and `logs path|tail` are right while that copy is running.
- `desk join` waits with no time limit.
