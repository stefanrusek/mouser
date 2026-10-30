# Contract: service ↔ session-engine IPC (Windows installed, v2)

These are the two sides of the contract:

- **Producer:** `apps/mouser-service-win` (`MouserService`, LocalSystem).
- **Consumer:** the session engine, which is `mouser-app.exe --headless --machine` running in the console session with a winlogon token. Its Windows backend owns the client side, in `crates/mouser-platform-windows/src/service_client.rs`.

The types live in the contract crate, in `crates/mouser-proto/src/service_ipc.rs`. The module is plain serde types and is OS-neutral, so it compiles everywhere. It is a new module in a frozen crate, created in the 002 Foundational phase. Both sides depend on it, and each side has its own contract test, `contract_service_ipc.rs`, that round-trips the frames against these types.

## Transport

- Named pipe: `\\.\pipe\mouser-svc-<console session id>`. The service creates it before it launches the engine.
- Flags: `PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED` and `PIPE_TYPE_BYTE | PIPE_REJECT_REMOTE_CLIENTS`. (Overlapped added 2026-10-30, orchestrator: the service writes `Shutdown` while its reader waits in `ReadFile`, which synchronous pipe I/O would serialize; it also lets a pending connect be cancelled.)
- Security descriptor: `D:P(A;;GA;;;SY)`, so only SYSTEM can connect.
- Client check: after `ConnectNamedPipe`, the service calls `GetNamedPipeClientProcessId` and requires the result to equal the PID it launched. Any other client is disconnected.
- Framing: 4-byte big-endian length, then a postcard body. This is the same framing as `mouser_proto::wire`.

## Messages

| Direction | Message | Meaning |
|---|---|---|
| engine → service | `Hello { version: String }` | Sent first. A version mismatch with the service's own version makes the service log it. It still serves `SendSas`. |
| engine → service | `SendSas` | Asks for Ctrl-Alt-Del. The service calls `SendSAS(FALSE)`. |
| service → engine | `SasResult(Sent \| PolicyBlocked \| Failed(String))` | `PolicyBlocked` means `SoftwareSASGeneration` is not 1 or 3 when read. The engine's `secure_attention()` returns `Err(Os(..))` for any outcome other than `Sent`. |
| service → engine | `Shutdown` | The service is stopping or uninstalling, or the console session changed. The engine runs `Engine::shutdown` (research V4) and exits. |
| engine → service | `Stopping { reason: StopReason }` (`StopReason = Quit \| Fault`) | Sent before a voluntary exit, so the service can log it. The service starts the engine again in either case, unless the service itself is stopping. |

## What does not travel on this pipe

Log bundles never travel on the service pipe and the service never writes a bundle. The session engine reads `service.log` from the machine folder when it gathers its own part, and hands the finished zip to the user's own Mouser over the control pipe (`logs.collect_stream`, control-api-delta.md; owner decision OD-7).

The service leaves `WaitingForSession` only on a `SERVICE_CONTROL_SESSIONCHANGE` after which `WTSGetActiveConsoleSessionId` returns a real session id. It never polls.

## Lifecycle

```text
service start → wait console session (≠0xFFFFFFFF) → create pipe(session) → launch engine(winlogon token, winsta0\default)
engine exits ─▶ log service.engine_exited ─▶ relaunch (same session)
SESSIONCHANGE (console connect/disconnect, logon/logoff) ─▶ Shutdown old engine ─▶ wait exit ─▶ new pipe/session ─▶ launch
SERVICE_CONTROL_STOP / PRESHUTDOWN ─▶ Shutdown ─▶ wait exit ─▶ stop
```

The service relaunches the engine as soon as it exits. This is not a retry after a timeout, and there is no back-off. A crash loop is visible in `service.log` as repeated `service.engine_exited` records.

## Tests

| Package | What is tested |
|---|---|
| service-win | The pipe security descriptor string. Rejection of a client whose PID does not match. Each lifecycle transition, through a `WinApi` seam (fake session ids, fake process handles). `SendSas` mapped to each policy value. |
| platform-windows | The client handshake. `secure_attention()` mapping each `SasResult`. A dropped pipe (service gone) makes `secure_attention()` return `Err(Os)`, and nothing else. |
