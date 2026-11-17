# Data model: cursor guard (macOS backend)

All of this is private to `crates/mouser-platform-macos`; nothing here crosses a crate seam except the one log event (contracts/log-events-delta.md).

## Expectation (what Mouser wants the Mac cursor to be)

| Field | Meaning |
|---|---|
| `phase` | `Remote` (grabbed: hidden, still, disassociated), `Arrived` (ungrabbed within the settle window: visible, associated), `Idle` (nothing to enforce) |
| `park` | the point the cursor was parked at when the grab began (`Remote`), or the landing point (`Arrived`) |
| `hides` | how many hides Mouser has issued and not yet shown (0 or 1 while it works) |
| `arrived_at` | when `Arrived` began (settle window: 1000 ms) |

State transitions: `Idle` -> `Remote` on `set_grabbed(true)`; `Remote` -> `Arrived` on `set_grabbed(false)`; `Arrived` -> `Idle` after 1000 ms or on `stop()`. `set_grabbed` with an unchanged value stays a no-op.

## Observation (what the guard read this tick)

`visible: bool`, `position: (f64, f64)`, `at: Instant`. Association is not observable; it is re-asserted instead (contract: macos-cursor-guard.md).

## Deviation and Correction

`decide(Expectation, Observation, own_echo) -> Vec<Correction>` is a pure function.

| Phase | Deviation | Correction |
|---|---|---|
| `Remote` | `visible` | show `hides`, then hide once; associate(false) |
| `Remote` | `position` differs from `park` by more than 1 point and is not Mouser's own echo | warp to `park`; associate(false) |
| `Arrived` | not `visible` | show; associate(true) |
| `Arrived` | position differs from `park` while no user motion was seen | none (the user moves the cursor; only visibility and association are enforced) |
| `Idle` | any | none |

A correction Mouser applies is recorded in the `Echo` ring (existing `echo.rs`) so the next tick does not read its own warp as foreign (FR-004). A deviation with an empty correction list is not logged.

## ForeignChange (log record)

`aspect` in {`visible`, `associated`, `position`}, `corrected: bool`, `count`. Coalesced per aspect over 1 s. `associated` is reported only when a re-assert follows a deviation in `visible` or `position` in the same tick (it cannot be read directly).
