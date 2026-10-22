use crate::cursor::*;

fn pointer() -> std::sync::MutexGuard<'static, ()> {
    crate::live_input()
}

#[test]
fn hiding_blanks_every_shape_and_showing_restores_the_scheme() {
    let _guard = pointer();
    recover().unwrap();
    assert!(!arrow_is_blank());
    hide().unwrap();
    assert!(arrow_is_blank());
    show().unwrap();
    assert!(!arrow_is_blank());
}

#[test]
fn hiding_twice_and_showing_twice_are_harmless() {
    let _guard = pointer();
    recover().unwrap();
    hide().unwrap();
    hide().unwrap();
    assert!(arrow_is_blank());
    show().unwrap();
    show().unwrap();
    assert!(!arrow_is_blank());
}

#[test]
fn showing_what_was_never_hidden_leaves_the_cursors_alone() {
    let _guard = pointer();
    recover().unwrap();
    show().unwrap();
    assert!(!arrow_is_blank());
}

#[test]
fn recovery_undoes_a_hide_left_behind_by_a_crash() {
    let _guard = pointer();
    hide().unwrap();
    // A crashed run never called show(); the next launch recovers.
    recover().unwrap();
    assert!(!arrow_is_blank());
    show().unwrap();
}

#[test]
fn a_failure_part_way_restores_everything_and_leaves_the_pointer_shown() {
    let _guard = pointer();
    recover().unwrap();
    let mut done = 0;
    let err = hide_with(&mut |_| {
        done += 1;
        if done == 3 {
            Err(mouser_platform::Error::Os("SetSystemCursor: denied".into()))
        } else {
            Ok(())
        }
    });
    assert_eq!(
        err,
        Err(mouser_platform::Error::Os("SetSystemCursor: denied".into()))
    );
    assert_eq!(done, 3, "stops at the first failure");
    assert!(!arrow_is_blank());
    // Not recorded as hidden, so a later hide tries again.
    hide().unwrap();
    assert!(arrow_is_blank());
    show().unwrap();
}

#[test]
fn startup_leaves_a_pointer_this_process_is_hiding_alone() {
    let _guard = pointer();
    recover().unwrap();
    hide().unwrap();
    recover_at_startup().unwrap();
    assert!(arrow_is_blank());
    show().unwrap();
    recover_at_startup().unwrap();
    assert!(!arrow_is_blank());
}
