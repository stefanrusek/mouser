use mouser_platform::LocalInput;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::DisplayId;

use super::state::*;

fn remote(c: u8) -> Control {
    Control::Remote(Remote {
        computer: ComputerId([c; 16]),
        display: DisplayId {
            computer: ComputerId([c; 16]),
            key: "d".into(),
        },
        at: (0.0, 0.0),
        speed: (1.0, 1.0),
        back: (5.0, 5.0),
    })
}

#[test]
fn a_session_starts_local_with_nothing_held() {
    let mut s = Session::default();
    assert_eq!(s.control, Control::Local);
    assert_eq!(s.remote(), None);
    assert!(s.remote_mut().is_none());
    assert!(s.held.is_empty() && s.sent.is_empty());
}

#[test]
fn physical_keys_and_buttons_are_tracked_and_motion_is_not() {
    let mut s = Session::default();
    s.track(LocalInput::Key {
        usage: 4,
        down: true,
    });
    s.track(LocalInput::Button {
        button: 1,
        down: true,
    });
    s.track(LocalInput::Key {
        usage: 5,
        down: true,
    });
    s.track(LocalInput::Key {
        usage: 5,
        down: false,
    });
    s.track(LocalInput::Motion {
        dx: 1.0,
        dy: 1.0,
        abs: (1.0, 1.0),
    });
    s.track(LocalInput::Scroll { dx: 1.0, dy: 1.0 });
    assert_eq!(s.held.keys.iter().copied().collect::<Vec<_>>(), [4]);
    assert_eq!(s.held.buttons.iter().copied().collect::<Vec<_>>(), [1]);
    s.track(LocalInput::Key {
        usage: 4,
        down: false,
    });
    s.track(LocalInput::Button {
        button: 1,
        down: false,
    });
    assert!(s.held.is_empty());
}

#[test]
fn leaving_this_computer_releases_what_is_physically_held() {
    let mut s = Session::default();
    s.track(LocalInput::Key {
        usage: 0xE1,
        down: true,
    });
    s.track(LocalInput::Button {
        button: 1,
        down: true,
    });
    s.sent.keys.insert(9);
    let Leave::Local(release) = s.leave() else {
        panic!("leaving the local computer");
    };
    assert_eq!(release.keys.iter().copied().collect::<Vec<_>>(), [0xE1]);
    assert_eq!(release.buttons.iter().copied().collect::<Vec<_>>(), [1]);
    assert!(
        s.sent.is_empty(),
        "nothing counts as sent after a transition"
    );
    // The keys are still physically down.
    assert!(!s.held.is_empty());
}

#[test]
fn leaving_a_remote_computer_has_it_release_everything() {
    let mut s = Session {
        control: remote(3),
        ..Session::default()
    };
    s.sent.keys.insert(4);
    s.sent.buttons.insert(1);
    assert_eq!(s.remote(), Some(ComputerId([3; 16])));
    assert_eq!(s.remote_mut().unwrap().computer, ComputerId([3; 16]));
    assert_eq!(s.leave(), Leave::Remote(ComputerId([3; 16])));
    assert!(s.sent.is_empty());
}

#[test]
fn sequence_numbers_only_grow() {
    let mut s = Session::default();
    assert_eq!((s.next_seq(), s.next_seq(), s.next_seq()), (1, 2, 3));
}
