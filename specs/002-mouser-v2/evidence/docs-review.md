# docs/ review (T505), 2026-10-30

Reviewed docs/cli.md, configuration.md, product-requirements.md, releasing.md, technical-decisions.md and troubleshooting.md against the code at HEAD (e90596c) and specs/002-mouser-v2. Corrections made (not "no drift"):

- troubleshooting.md: removed "Windows backend does not exist" and "crossings are not logged"; added elevated_app, the v2 limitation texts, log header, event catalog, rotation (two files of max_log_mb/2), `mouser logs collect`, and that uninstall keeps C:\ProgramData\Mouser. Corrected the stale claims that a bad config.toml stops Mouser at start, that a missing key invalidates the file, and that hotkeys.enable_disable is unchecked (code: serde defaults, load() falls back to defaults, one shared validator).
- configuration.md: Windows paths and machine folder, new keys, headless on all three OSes, same stale config-start claims, removed the "known gap" for offline hotkey validation.
- cli.md: logs collect, desk rename, config set origin, v2 status text/JSON, Windows endpoint choice, app and service command lines; stale version examples and send-cad text.
- releasing.md: service sidecar and installer plan (marked not built), unsigned install instructions, "Not included yet" list.
- technical-decisions.md: stale "empty/stub" crates, workspace version, Windows signing; added as-built V1-V20 summary and P1-P4 outcomes.
- product-requirements.md: Windows backend exists (was "not delivered"), v2 note on every item, two new limitations, limitation 4 superseded for installed Windows, Windows portable zip exists.

Known remaining issues, not fixed:
- The Help section renders a Markdown subset without tables (ui/markdown.js), so the existing tables in troubleshooting.md show as run-together paragraphs in Help. New content uses lists; converting the old tables was out of scope.
- Not confirmed from code or hardware: the installed Windows log location (C:\ProgramData\Mouser\logs), the ACL on the machine secrets folder after a real install (owner row 15), the P2/P4 items, and that the window's Collect button works against MouserService (T530-T532 pending).
- Statements about the per-machine installer describe the plan (T490-T493 pending); the version is still 0.2.8 until T494.
