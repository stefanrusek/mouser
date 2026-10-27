use std::sync::{Arc, Mutex};

use crate::lock::LockExt;

#[test]
fn guard_returns_the_guard_of_an_ordinary_mutex() {
    let mutex = Mutex::new(5);
    *mutex.guard() += 1;
    assert_eq!(*mutex.guard(), 6);
}

#[test]
fn guard_survives_a_thread_that_panicked_while_holding_the_lock() {
    let mutex = Arc::new(Mutex::new(vec![1]));
    let holder = mutex.clone();
    let panicked = std::thread::spawn(move || {
        let mut held = holder.lock().unwrap();
        held.push(2);
        panic!("fault while holding the lock");
    })
    .join();
    assert!(panicked.is_err());
    assert!(mutex.is_poisoned());
    // The plain `lock().unwrap()` would panic here.
    assert_eq!(*mutex.guard(), vec![1, 2]);
    mutex.guard().push(3);
    assert_eq!(*mutex.guard(), vec![1, 2, 3]);
}
