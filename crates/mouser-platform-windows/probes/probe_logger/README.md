# probe_logger

Standalone probe, outside the workspace and the gate: repeats the capture watchdog's probe (a tagged one-pixel move and F24 release through `SendInput`) every 5 s and logs, per probe, the send result, how long each LL hook took to see it, `GetLastInputInfo`, `GetTickCount`, the cursor and the foreground window to `%TEMP%\probe_logger.log`. It is the evidence behind "A stalled guest looks like lost hooks" in `../FINDINGS.md`. Run with `cargo run --release -- [seconds]` (default 360); stop the installed mouser-app first so the two sets of hooks do not interfere.
