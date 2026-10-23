//! Which computer a `mouser status --json` reports as the desk's active one.

/// Whether `status` reports `id` as the active computer. A computer's own
/// status says `active_computer: null` while its own keyboard and pointer are
/// in use, so `own` (the observer is the controlled computer) matches null too.
pub fn is_active(status: &serde_json::Value, id: &str, own: bool) -> bool {
    status["active_computer"] == id || (own && status["active_computer"].is_null())
}
