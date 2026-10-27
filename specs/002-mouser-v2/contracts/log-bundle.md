# Contract: desk log bundle (v2)

This contract covers Story 2 sc. 10, Story 3 sc. 12 and FR-007. The bundle is produced by `logs.collect` (see control-api-delta.md), which the CLI runs as `mouser logs collect` and the UI as Diagnostics → "Collect logs from the desk".

## File

- **Name:** `mouser-logs-<desk>-<yyyymmddThhmmssZ>.zip`. `<desk>` is the desk name with every character outside `[A-Za-z0-9_-]` replaced by `-`, or `no-desk` when the computer is on no desk.
- **Location:** the `dir` param if one is given. Otherwise the user's Downloads folder, or the machine folder's `bundles/` subfolder for a headless `--machine` engine on macOS or Linux. If a file with that name already exists, ` (2)`, ` (3)` and so on is added before `.zip`.
- **Windows installed (mode `service`, owner decision OD-7):** the SYSTEM session engine writes no file. It returns the zip over the control pipe (`logs.collect_stream`, control-api-delta.md), and the user's own Mouser app or CLI writes it, as the user, to the folder the user chose or the user's Downloads folder, with the same name and collision rule.
- **Delivery:** the file is written only on the requesting computer. Nothing is sent to any other computer or to the internet (FR-007, constitution III).

## Layout

```text
bundle.json                         # log-bundle.schema.json
<name>-<id[0..8]>/computer.json     # the LogsCard + connection as seen by the requester
<name>-<id[0..8]>/config.toml       # verbatim
<name>-<id[0..8]>/mouser.log        # when included
<name>-<id[0..8]>/mouser.log.1      # when it exists
<name>-<id[0..8]>/service.log       # Windows installed: the service log, when included
```

## Rules

- The requesting computer is always included. It is listed first.
- `included` is false only with a `reason`:
  - `offline`: the computer was not connected, or its link was lost while sending.
  - `logging_off`: the computer's card and config are still present.
  - `older_version`: the computer runs a version below 0.3.0; only its card is present, built from what the requester knows about it.
- **Never present** (checked by `contract_log_bundle` and acceptance test SC-004): `state.json`, any file from `secrets/`, the desk key, the device key, and any line containing the typing or clipboard marker.
