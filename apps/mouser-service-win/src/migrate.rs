//! Identity migration during `--install` (research V12a, OD-2): the installed
//! Mouser's keys are already machine scope (DPAPI in the machine folder's
//! `secrets`), so what moves is the installing user's desk membership,
//! `state.json`, checked against the keys it needs.

use std::path::Path;

use mouser_core::engine_headless::{SecretStore, export_identity};

/// Copies the identity in `user_dir` and the installed store (`store` opens
/// it) to the machine folder `machine`; the reason when it cannot.
pub fn migrate(
    user_dir: &Path,
    machine: &Path,
    store: &dyn Fn() -> Box<dyn SecretStore>,
) -> Result<(), String> {
    export_identity(user_dir, &*store(), machine, &mut *store())
}
