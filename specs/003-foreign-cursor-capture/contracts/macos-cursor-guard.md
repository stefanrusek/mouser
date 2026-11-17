# Contract: macOS cursor guard (internal to `mouser-platform-macos`)

Fixed here so the tasks can order the `Cursor` trait change before the guard and the tests that use it.

```rust
trait Cursor: Send + Sync {            // existing; additions marked
    fn position(&self) -> (f64, f64);
    fn warp(&self, x: f64, y: f64) -> Result<()>;
    fn associate(&self, on: bool) -> Result<()>;   // was infallible with expect(): now returns the OS refusal
    fn hide(&self) -> Result<()>;                  // same
    fn show(&self) -> Result<()>;                  // same
    fn visible(&self) -> bool;                     // NEW: CGCursorIsVisible
}
```

Timing constants (one place, `cursor_guard.rs`): `TICK = 20 ms`, `REASSOCIATE = 250 ms`, `SETTLE = 1000 ms`, `COALESCE = 1 s`.

Behavioural contract
1. The guard runs only between `set_grabbed(true)` and `SETTLE` after `set_grabbed(false)`; `stop()` and `Drop` end it and join the thread.
2. It takes no lock the tap callback takes; it never blocks forwarding (FR-010).
3. Every correction restores the cursor to Mouser's expected state within one `TICK` of being read (FR-001/SC-002: at most `TICK` plus read latency, target < 45 ms).
4. A refused OS call inside the guard is logged and retried next tick; it never panics (002 V1) and never leaves `hides` unbalanced.
5. `set_grabbed(false)` leaves the cursor visible and associated, and after it `hides == 0` (rule 10 stays true).
6. Foreign motion: while grabbed, only HID-source motion becomes `LocalInput::Motion` (research D2).

The `InputCapture` / `LocalInput` trait in `crates/mouser-platform` is unchanged.
