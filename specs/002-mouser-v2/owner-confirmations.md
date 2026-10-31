
| 16 | Mac (real hardware) | `cargo test -p mouser-platform-macos --features real-drag --test contract_platform_macos_us7`, hands off the mouse | Four `ok` lines and exit 0. Not in hosted CI: macOS 15/26 runners never deliver mouse events to the drag loop. Passed on the owner's Mac 2026-10-31 | us7 CI exception |
