# Data Model: Mouser v1

Entities from spec.md Key Entities, with fields, validation rules, relationships and state transitions. Persistence is the per-computer config file (`contracts/config.md`) plus a per-computer state file; nothing is stored centrally. Wire representations are in `contracts/wire-protocol.md`.

## Identity

### ComputerId
- 128-bit random value generated on first launch, persisted in local state. Never derived from hostname, MAC or IP (those change; Story 3).
- Paired with a long-term Ed25519 **device key**. ComputerId = first 16 bytes of SHA-256(device public key), so identity is self-certifying.

## Desk

| Field | Type | Rules |
|---|---|---|
| desk_id | 128-bit random | Assigned at creation; immutable. |
| name | string | 1–64 chars, trimmed, Unicode; not unique on the network (edge case: same names are distinguished by member names). |
| members | set of Member | ≥1. |
| allowlist | optional set of IP networks | When present, join requests from addresses outside it are refused (Story 1 sc. 7). |
| desk_key | 32-byte secret | Generated at creation; shared with a member only after approval; used to authenticate desk traffic and derive per-session keys (FR-002). Rotated when a member is removed. |
| layout | Layout | Exactly one per desk. |
| epoch | u64 | Monotonic version of membership + layout + desk settings; last-writer-wins by (epoch, ComputerId) tie-break. |

**Member**: computer_id, device_public_key, display_name, os, mouser_version, joined_at.

State transitions (per computer, `DeskMembership`):

```text
Unjoined ──create──▶ Member(desk)
Unjoined ──request(desk)──▶ Pending(desk) ──approved──▶ Member(desk)
                                    └──declined/timeout──▶ Unjoined (user told "not admitted")
Member(desk) ──leave / removed──▶ Unjoined   (desk_key rotated by remaining members)
Member(desk A) ──move to B──▶ Pending(B)     (leaves A on approval of B)
```

## Computer (runtime view of a peer)

| Field | Type | Rules |
|---|---|---|
| computer_id | ComputerId | |
| display_name | string | OS computer name by default; owner may rename. |
| os | enum {macos, windows, linux} + version string | |
| mouser_version | semver | Compatibility decided by protocol version (wire-protocol.md). |
| connection | ConnectionState | See below. |
| permissions | map PermissionKind → {granted, missing, not_applicable} | PermissionKind: accessibility, input_monitoring, screen_recording, full_disk_access (macOS); input_capture_portal, remote_desktop_portal (Linux Wayland); none on Windows/X11. |
| limitations | list of Limitation | FR-001: platform limitations shown in app. |
| last_seen | timestamp | Updated on any authenticated packet. |
| secure_input_holder | optional {pid, app_name} | macOS only (Story 2 sc. 10). |

ConnectionState transitions:

```text
Discovered ─handshake ok─▶ Connected ─path lost─▶ Reconnecting ─path regained─▶ Connected
     │                          │                      └─(>grace)─▶ Offline ─rediscovered─▶ Connected
     └─incompatible version─▶ Incompatible            Connected ─peer leaves desk─▶ (removed)
```
While Reconnecting/Offline and the peer was the active computer: control returns to the local computer (Story 3 sc. 5).

## Display

| Field | Type | Rules |
|---|---|---|
| display_id | (ComputerId, platform display key) | Platform key is stable across reboots where the OS provides one (EDID/serial-based), else index. |
| name | string | From OS. |
| pixel_size | (w, h) | |
| logical_size | (w, h) | pixel_size / scale. |
| scale | f32 | ≥1.0. |
| physical_size_mm | optional (w, h) | Used by equalized dimensions (Story 4 sc. 4) and DPI-consistent speed (Story 2 sc. 5). |
| origin_local | (x, y) | Position in the owning computer's own desktop coordinates. |
| included | bool | false = removed from layout (Story 4 sc. 3). |

## Layout

- `placements`: map display_id → (x, y) in **desk space**, integer millimetre-like units derived from physical size when known, else logical pixels at 96 dpi equivalent.
- Edges are derived, not stored: two included displays adjoin where their desk-space rectangles share a boundary segment of length > 0.
- `edge_options`: map (display_id, side) → { block_corners: bool (default true) , corner_size: fixed default }.
- `equalize_dimensions`: bool (default false).
- `block_fullscreen`: bool (default true).
- Rigid groups: the included displays of one computer keep the relative arrangement the platform reports (`origin_local`, each at its own density, edges they share locally are shared in desk space). Their placements differ from that arrangement only by one common translation; the owner moves a computer as a whole, never one display away from its siblings. The wire shape (`placements` per display) is unchanged; core validation refuses a layout that breaks the rule.
- When a computer's displays change (added, removed, moved in the OS), the group is re-arranged to the new OS arrangement and anchored on the persisting display that is first in the OS arrangement (smallest `origin_local`), so it keeps its place relative to the other computers; if that would overlap another computer it is auto-placed afresh.
- Validation: placements of included displays MUST NOT overlap; every included display MUST be reachable (graph connected) — the arranger refuses a drop that would disconnect a display.
- Auto-placement (Story 2 sc. 1): a new computer's displays, in their local arrangement, are appended to the right of the layout: its leftmost display adjoins the right side of the rightmost placed display, top-aligned with it, so the two computers always share an edge. Displays that share an edge locally share it in desk space, whatever their densities. A computer whose displays share no edge with any other computer's is placed again the same way. A `needs_review` flag is set and surfaced to the owner.

## ActiveSession (who has control)

| Field | Rules |
|---|---|
| active_computer | ComputerId; the computer receiving input. Exactly one per desk at a time. |
| controller | ComputerId whose physical devices are being shared. |
| pointer | (display_id, x, y) in the active display's logical coordinates. |
| pressed | set of held keys + buttons, so releases can be synthesized on switch/loss (edge case: no stuck keys). |
| seq | u64 per-session input sequence number (Story 3 sc. 1 ordering). |

Transitions: `Local → Remote(c)` on crossing/QuickJump; `Remote(c) → Local` on panic key, crossing back, or loss of c; `Remote(c) → Remote(d)` on crossing between remote displays. Every transition releases all `pressed` on the computer being left.

## ClipboardContent

- `origin`: ComputerId, `captured_at`, `sequence` (monotonic per origin).
- `representations`: ordered list of { kind ∈ {text/plain, text/html, text/rtf, image/png, files}, bytes or file manifest }.
- Files are transferred lazily: only a manifest (names, sizes, relative paths) is sent on copy; bytes are fetched on paste (Story 6 sc. 2), reusing Transfer.
- Targeted paste (Story 6 sc. 3) reads the remote content without writing to other computers' clipboards.

## Transfer

| Field | Rules |
|---|---|
| transfer_id | random |
| source, destination | ComputerId |
| items | list of {relative_path, size, is_dir} |
| destination_dir | absolute path on destination |
| progress | bytes_done, bytes_total, current_item, rate, eta |
| state | `Pending → Checking (space) → Copying → Completed` ; `Checking → Rejected(no_space)` ; `Copying → AwaitingOverwriteDecision → Copying` ; `Pending/Checking/Copying/AwaitingOverwriteDecision → Failed(peer_offline)` |

Validation: total size is compared with free space before any byte is written (Story 7 sc. 4); a name conflict pauses that item for the user's choice (Story 7 sc. 3).

Cancelled drop (owner decision 2026-10-22): if the user cancels the incoming drag on the destination (Esc, or a drop on something that is not a folder; the platform `FileDrag::begin_incoming` returns an error), the transfer ends quietly with nothing copied and no new state. The destination forgets the transfer while it is still `Pending`, publishes no `transfer` notification (none had been published, so no progress display opens or stays), raises no Problem, and sends `TransferDone` so the source forgets it as if the files had been dropped back where they started (the source, like for any `TransferDone`, publishes nothing). No wire change. Future item (v2, owner wish): put the files in a scratch folder instead.

Interruption (orchestrator decision 2026-10-21; the spec is silent): if the other computer goes offline mid-transfer, both sides move the transfer to `Failed(peer_offline)` and publish a `transfer` notification in state `failed` with `reason: "peer_offline"` and a plain-language `message` naming the computer and how many items were completed (e.g. "The copy stopped because Beta went offline. 2 of 5 files were copied."). The destination deletes the partially written item (completed items are kept). Nothing is retried automatically.

## Problem

| Field | Rules |
|---|---|
| problem_id | stable kind + subject key, so the same problem is not duplicated |
| kind | enum: missing_permission, incompatible_version, join_refused, secure_input, crossing_blocked, peer_offline, platform_limitation |
| computer | ComputerId |
| message | plain-language text (constitution II) |
| fix | optional FixAction: open_system_settings(kind), approve_join, open_update, cross_anyway_hint, open_arranger_edge |
| raised_at, cleared_at | |

Shown in health panel (Story 8) and, for kinds the spec lists, as a notification.

## Settings (per computer, plus desk-shared)

Desk-shared (replicated with `epoch`): layout, edge options, equalize, block_fullscreen, allowlist.
Per computer: hotkeys (enable/disable, QuickJump, targeted paste, panic key = Esc×2 default), buttons_4_5 ∈ {passthrough, back_forward}, ports (Advanced), diagnostic_logging bool, start_at_login bool, update_channel (stable), notifications bool.
Every setting has exactly one key in the config file (FR-005; contracts/config.md).

## LogRecord

One JSON object per line (FR-010): `ts` (RFC 3339 UTC), `level`, `computer` (ComputerId + name), `event` (dotted name), `fields` (object). Schema in `contracts/log-record.schema.json`.

## Release

`version` (semver), `pub_date`, `min_protocol`, `platforms` map "<os>-<arch>" → {url, minisign signature} (contracts/release-manifest.schema.json, the tauri-plugin-updater `latest.json` format). Downloaded by one desk computer, which verifies every platform's signature and then serves the manifest and bundles to members over the desk connection (Story 10, FR-003). A computer that installed a release from a desk-mate keeps its own platform's verified bundle too and offers it to older desk-mates after restarting. The fetcher postpones its own install while offered desk-mates are still fetching: 5 s for each to start (`FETCH_START_GRACE`), 60 s at most overall (`FETCH_SETTLE_LIMIT`).
