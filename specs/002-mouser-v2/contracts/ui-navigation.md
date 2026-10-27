# Contract: Mouser window navigation (v2, Story 3)

There are two sides to this contract. The **Rust app** (`apps/mouser-app/src/windows.rs`, `tray.rs`) produces navigation requests. The **UI** (`apps/mouser-app/ui/index.html` + `ui/nav.js`) consumes them. Both sides test against this document with `contract_`-prefixed tests (constitution 4.1): `apps/mouser-app/src/contract_navigation_tests.rs` on the Rust side (producer: `windows.rs` navigation and the tray's navigate calls; `tray_model_tests.rs` is renamed `tray_tests.rs` in 002 and keeps the unit tests) and `apps/mouser-app/ui/contract_navigation.test.js` on the UI side (consumer: routes, sidebar, `mouser://navigate`).

Layout follows `reference/fluid-voice-style.png` (FR-011): one window with a search field at the top of a sidebar, sections grouped under headings, and a home overview as the first section. Colours, fonts and wording are our own.

## Window

- There is one normal window, labelled `main`, at `index.html` (default 960×680, resizable, minimum 720×520).
- The `landing` and `quickjump-*` overlays are unchanged (they are not navigable).
- The windows `settings`, `arranger`, `health`, `join` and `transfer`, and the pages `first_run.html`, `desk_settings.html`, `input_settings.html`, `health.html`, `arranger.html`, `join_prompt.html` and `transfer.html`, are **removed**. Their content becomes the sections and panels listed below.
- Closing the window hides it. It is reused on the next open.

## Routes (location hash)

| Route | Sidebar heading → item | Content (spec) |
|---|---|---|
| `#desk` (default) | Your desk → Desk | Desk name. For each computer: friendly name, OS, version (both versions when different), connection, whether it has the pointer, and a problem count that links to `#health/<id>`. Members, remove, leave or move the desk. Join approval card. Transfer progress panel. (S3 sc. 4, 8) |
| `#displays` | Your desk → Displays | Arranger with full-name labels; the placement-review message with "Keep this placement"; edge and display options; desk-wide `block_fullscreen` and `equalize_dimensions` (S3 sc. 10–11) |
| `#health`, `#health/<computer id>` | Your desk → Health | One entry per computer: connection, responsiveness rounded to whole ms, permissions (this computer: granted or not, plus "Open settings"), problems (once, each with its fix), "What doesn't work here" (limitations). The route scrolls to and highlights that computer's entry. (S3 sc. 5–7, 9, 16; S5 sc. 3, 8; S6 sc. 2) |
| `#settings`, `#settings/<key>` | Configure → Settings | Friendly names (any computer), panic key (key recorder plus press count), QuickJump / enable-disable / targeted-paste recorders, landing highlight, "Use Command as Control on Windows and Linux", mouse buttons 4/5, start at login, notifications, Advanced (port, allowlist). The `<key>` route focuses that control. (S4) |
| `#diagnostics` | Support → Diagnostics | Logging switch, "Open log location", "Collect logs from the desk" (shows the result and reveals the file) (S3 sc. 12) |
| `#help` | Support → Help | This computer's Mouser version and the shipped troubleshooting guide (S3 sc. 17) |
| `#welcome` | (no sidebar item; sidebar hidden) | First run: permission step and desk step (001 Story 1 sc. 1). When it finishes, the window goes to `#desk`. (S3 sc. 9) |

## Requests from Rust to the window

- **Event:** `mouser://navigate` with payload `{route: string}`. Rust first calls `unminimize`, `show` and `set_focus` on `main`, or creates `main` at `index.html#<route>` if it does not exist.
- **Callers:**
  - The tray: "Open Mouser" goes to `#desk`, or `#welcome` while setup is needed. "Arrange displays" goes to `#displays`. "Mouser health" goes to `#health`.
  - A second launch goes to `#desk`.
  - A join request goes to `#desk`.
  - A transfer question goes to `#desk`.
  - `ReviewOffered` goes to `#displays`.
  - The crossing-blocked fix goes to `#displays`.
  - The `elevated_app` toast goes to `#health/<id>`.
- **Rule:** no caller ever creates a second normal window (S3 sc. 2).

## Search index (`ui/search_index.js`)

- **Entries:** a static array of `{route, title, section, keywords[]}`. It has one entry per section and one per setting control (a `#settings/<key>` or `#displays/<key>` route).
- **Matching:** case-insensitive substring match on the title and the keywords. Results are listed under the search box, and choosing one navigates to its route (S3 sc. 3).
- **Completeness check:** a contract test asserts two things. First, every config key reachable in the UI has an entry. Second, every route is at most two clicks from any section: one sidebar click, then at most one in-section control (SC-005).

## Tray (Rust, Story 3 sc. 13–15)

| Input | Action |
|---|---|
| left-click up | toggle pause (`active.pause` / `active.resume`) |
| right-click | open the menu |
| Linux | menu only, with the stated limitation |

**Icon:** chosen from `tray_summary.state` × `tray_summary.connection` (research V18). The tooltip text names both states, for example "Mouser is paused · not connected to Beta".

**Menu items:** the summary, the version/update line, Open Mouser, Pause/Resume, Arrange displays, Mouser health, Send Ctrl-Alt-Del (when available), and Quit. Quit runs full cleanup (research V4).
