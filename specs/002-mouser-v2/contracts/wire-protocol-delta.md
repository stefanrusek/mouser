# Contract delta: wire protocol (v2)

This file lists only the changes from `specs/001-mouser-v1/contracts/wire-protocol.md`, which still applies in full: ALPN `mouser/1`, framing, `PROTOCOL_VERSION = 1` and every message it defines.

> **Frozen-crate change.** This contract changes `crates/mouser-proto/src/wire.rs`, which is frozen. The change is made in the 002 Foundational phase under this approved plan and needs orchestrator/owner approval before it lands (the same route T517/`Paused` took).

## Version gating

- `V2_SINCE = "0.3.0"`.
- Every message added below goes at the **end** of `ControlMessage`, after `Paused`. Existing postcard variant indexes do not change.
- A computer sends these messages only to peers whose `Hello.mouser_version` is ≥ `V2_SINCE`. The comparison uses `release::Version`, as `crossing/pause.rs:104-107` does today.
- 0.2.x peers never receive them, because those peers panic on an unknown variant (`transport/mod.rs:352`).
- `PROTOCOL_VERSION` stays 1, so 0.2.x and 0.3.x computers are compatible.

## Reader rule (new, 0.3.0 and later)

- If a frame has a valid length prefix but its postcard body names a `ControlMessage` variant index this build does not know, the reader **skips** the frame and logs `wire.unknown {variant}`. The link stays up.
- Any other decode failure closes the link with close reason `protocol`.
- Because of this rule, future additions do not need gating against peers that are 0.3.0 or later.

## New messages (appended in this order)

| Message | Fields | Direction / semantics |
|---|---|---|
| `LogsRequest` | `request: [u8; 16]` (random) | requester → each connected peer that is at least 0.3.0. The peer answers on a new **uni stream** (see "Logs stream" below). |
| `DeskSettings` | `epoch: u64`, `author: ComputerId`, `landing_highlight: bool`, `command_as_control: bool` | Sent to every authed peer that is at least 0.3.0 whenever these settings change, and after each new connection. The receiver adopts it if `(epoch, author)` is higher than what it holds, writes `config.toml`, and logs `setting.changed origin=desk_mate`. |
| `RestartElevated` | — | Sent by a desk computer to a Windows computer that is at least 0.3.0, portable and not elevated. That computer's engine raises `EngineEvent::RestartElevated`, and its app runs the elevated restart (research V19). It is ignored when the receiver is not portable Windows. |

## Logs stream

The logs stream reuses the release-stream routing:

- The routing is in `session/mod.rs::file_loop`, which matches uni streams by a 16-byte id against parked waiters.
- The stream id is `sha256("logs\n" ‖ request)[..16]`.

Stream layout:

```text
stream_id: [u8;16]
header_len: u32 BE
LogsPart (postcard) {
  card: LogsCard { id, name, os, os_version, version, mode },
  config_toml: String,            // verbatim; holds no secrets
  logging: bool,                   // false ⇒ files is empty, reason logging_off
  files: Vec<(String /*mouser.log | mouser.log.1*/, u64 /*len*/)>,
}
then for each file, `len` raw bytes in order; stream finish.
```

- Before answering, the peer checks that the requester is authed on this desk. This is the same check `ReleaseFetch` makes.
- The requester parks one waiter per asked peer.
- A peer's part is complete when the stream finishes. If the peer's link is lost first, that peer is recorded as `offline`. No timer is involved: link loss comes from 001 R2 liveness.

## Changed semantics (no shape change)

- **Close reasons.** New reason bytes `quit` (replaces `shutdown`), `fault`, `protocol` and `incompatible`. A 0.3.0 peer maps `shutdown` from a 0.2.x computer to `peer_quit`.
- **`DeskState.members[].display_name`.** This is now the desk-wide friendly name. Renaming increments `epoch` and broadcasts `DeskState` as before, so 0.2.x peers adopt it unchanged.
- **`Panic` from the computer being controlled.** The receiver sends `Panic` to its controller when the foreground app outranks Mouser (research V6). Under 001 semantics the controller returns to Local and sends `ReleaseAll`. This works for every peer version that is 0.2.6 or later.
- **`PeerStatus`** gains `mode` at the end. It is sent only to peers that are at least 0.3.0; 0.2.x peers get the 001 shape. Peers that are 0.3.0 or later no longer list start-up `platform_limitation:<capability>` problems.

## Contract tests (`crates/mouser-proto/tests/contract_wire.rs`, extended)

- Round-trip each new message.
- Old variant indexes are unchanged; an index snapshot is taken.
- An unknown-variant frame decodes to `FrameError::UnknownVariant(index)`, which is distinct from `Malformed`. This variant is a `mouser-proto` addition.
- `LogsPart` round-trips.
- `stream_id` derivation.
