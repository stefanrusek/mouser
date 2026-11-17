# Contract delta: diagnostic log events (003)

Amends `specs/002-mouser-v2/contracts/log-events.md` and `log-events.schema.json` (closed `fields` per event). Written by the macOS backend through its diagnostics sink with the catalog name below (research D5); no platform-trait change.

| event | level | peer | fields | Spec |
|---|---|---|---|---|
| `cursor.foreign_change` | warn | – | `aspect` (`visible` / `associated` / `position`), `phase` (`remote` / `arrived`), `corrected` (bool), `count` (changes of that aspect in the 1 s window that this record closes) | FR-007, S3 sc.1, SC-005 |

Rules
- Written at most once per aspect per second, when the window closes (or on ungrab/stop), so a game that re-captures every frame cannot flood the log.
- Never carries the foreign app's name or window title (the spec's privacy rule); a later version may add `app` using the existing foreground-app name path.
- Mouser's own corrections never produce a record (FR-004).
- `contract_log_events` and `contract_log_privacy` gain this row; the JSON schema gains the event with `additionalProperties: false`.

Reading it: `cursor.foreign_change {aspect: visible, phase: remote, corrected: true, count: 3}` answers "did another app change the cursor, and did Mouser correct it?" (SC-005).
