# Contract: Configuration File (v1)

Plain-text TOML at the per-OS config path (`~/Library/Application Support/Mouser/config.toml`, `%APPDATA%\Mouser\config.toml`, `$XDG_CONFIG_HOME/mouser/config.toml`; portable mode: `<exe dir>/MouserData/config.toml`). Every GUI setting has exactly one key here (FR-005). Mouser watches the file and applies changes without restart. Invalid values are rejected with `key: reason` (CLI) and the previous value stays in effect.

```toml
schema = 1

[general]
start_at_login = true            # FR-011
notifications = true
computer_name = ""               # empty = OS computer name

[hotkeys]                        # accelerator strings, e.g. "Ctrl+Alt+M"
enable_disable = "Ctrl+Alt+Shift+M"
quickjump = "Ctrl+Alt+Shift+J"
targeted_paste = "Ctrl+Alt+Shift+V"
panic = "Escape*2"               # key pressed twice

[mouse]
buttons_4_5 = "passthrough"      # "passthrough" | "back_forward"

[crossing]                       # desk-shared; replicated via DeskState
block_fullscreen = true
equalize_dimensions = false
# per-edge corner blocking lives in layout; default true

[diagnostics]
logging = false                  # FR-010
max_log_mb = 50

[updates]
automatic = true                 # Story 10
release_url = "https://api.github.com/repos/<owner>/mouser/releases/latest"

[advanced]
port = 47801                     # FR-008; UDP (QUIC)
allowlist = []                   # CIDR strings; empty = off
```

Desk-shared keys (`[crossing]`, `advanced.allowlist`, layout) written locally are propagated to the desk with a new epoch.
