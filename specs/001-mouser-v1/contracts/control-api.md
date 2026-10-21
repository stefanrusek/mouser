# Contract: Local Control API (v1)

The seam between the running Mouser engine (`mouser-core` inside the Mouser app process) and its two local clients: the Tauri UI and the `mouser` CLI. Transport: newline-delimited JSON-RPC 2.0 over a local socket — Unix domain socket `$XDG_RUNTIME_DIR/mouser.sock` (Linux), `~/Library/Application Support/Mouser/mouser.sock` (macOS), named pipe `\\.\pipe\mouser-<user SID>` (Windows). Only the same OS user may connect. The UI uses the same methods through Tauri commands that forward to this API in-process.

Schemas: `status.schema.json` (result of `status`). Both sides get `contract_` tests against these schemas.

| Method | Params | Result | Spec |
|---|---|---|---|
| `status` | — | Status (see schema) | Story 8, Story 9 sc. 1 |
| `desks.discovered` | — | [{desk_id, name, members:[name]}] | Story 1 sc. 2 |
| `desk.create` | {name} | Desk | Story 1 sc. 3 |
| `desk.join` | {desk_id} \| {address} | {state: "pending"} | Story 1 sc. 2, 6 |
| `desk.leave` | — | {} | Story 1 sc. 5 |
| `desk.move` | {desk_id} | {state: "pending"} | Story 1 sc. 5 |
| `desk.remove_member` | {computer_id} | {} | Story 1 sc. 5 |
| `join.pending` | — | [{request_id, computer, desk}] | FR-013 |
| `join.decide` | {request_id, approve} | {} | FR-013 |
| `layout.get` / `layout.set` | — / Layout | Layout | Story 4 |
| `config.get` | {key?} | value or full config | Story 9 sc. 2 |
| `config.set` | {key, value} | {} (validated as in config.md) | Story 9 sc. 2 |
| `logging.set` | {enabled} | {path} | Story 8 sc. 4, FR-010 |
| `active.pause` / `active.resume` | — | {} | Story 2 sc. 12 |
| `send_cad` | — | {} or error unsupported | Story 12 sc. 4 |
| `update.check` / `update.apply` | — | {current, available?} | Story 10 |
| `subscribe` | {topics:[...]} | stream of notifications: `problem`, `join_request`, `transfer`, `status` | UI live updates |

Errors use JSON-RPC error objects with `data.kind` from the Problem kinds in data-model.md.
