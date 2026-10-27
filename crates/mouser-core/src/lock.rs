//! Locks never poison (research V2): a panic while a lock is held must not
//! turn one fault into a permanently dead engine. State consistency after a
//! fault is restored by `Crossing::recover_after_fault`, not by poisoning.

use std::sync::{Mutex, MutexGuard, PoisonError};

/// Take a lock whether or not a thread panicked while holding it.
pub(crate) trait LockExt<T> {
    fn guard(&self) -> MutexGuard<'_, T>;
}

impl<T> LockExt<T> for Mutex<T> {
    fn guard(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
