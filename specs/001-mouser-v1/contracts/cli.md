# Contract: `mouser` CLI (v1)

Persona: troubleshooting agent (Story 9). Every command supports `--json` (machine-readable, schema-backed) and defaults to human-readable text. Exit codes: 0 success, 1 error, 2 Mouser not running, 3 invalid argument/value.

| Command | Behaviour |
|---|---|
| `mouser status [--json]` | Desk, members, connection state, permissions, limitations, problems. JSON = `status.schema.json`. |
| `mouser desks [--json]` | Desks discovered on the network. |
| `mouser desk create <name>` / `join <name\|desk_id\|host:port>` / `leave` / `remove <computer>` | Membership (join waits for approval and reports the decision). |
| `mouser approve <request>` / `decline <request>` / `mouser requests` | Pending join requests (FR-013). |
| `mouser config get [<key>]` / `config set <key> <value>` / `config path` | Read/write settings; `set` validates and reports `key: reason` on invalid values. |
| `mouser logs enable` / `disable` / `path` / `tail [-n N]` | Diagnostic logging (FR-010). `tail` prints raw JSON Lines. |
| `mouser pause` / `resume` | Story 2 sc. 12. |
| `mouser update [check\|apply]` | Story 10. |
| `mouser send-cad` | Story 12 sc. 4. |

When the app is not running, `config get/set/path` and `logs path/tail` operate directly on files; others exit 2 with "Mouser is not running".
