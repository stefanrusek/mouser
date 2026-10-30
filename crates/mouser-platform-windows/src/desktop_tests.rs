use crate::desktop::*;
use crate::diag::{DiagDesktop, DiagValue, heard_during};
use std::cell::RefCell;

/// A PC whose input desktop the test sets, recording what the follower did.
#[derive(Default)]
struct FakeDesks {
    /// The desktop that has the input; `None` when it cannot be opened.
    input: RefCell<Option<String>>,
    /// Whether switching this thread's desktop is refused.
    refuse: bool,
    done: RefCell<Vec<String>>,
}

impl FakeDesks {
    fn on(name: &str) -> FakeDesks {
        let desks = FakeDesks::default();
        desks.show(Some(name));
        desks
    }

    fn show(&self, name: Option<&str>) {
        *self.input.borrow_mut() = name.map(str::to_string);
    }

    fn done(&self) -> Vec<String> {
        std::mem::take(&mut self.done.borrow_mut())
    }
}

impl DesktopApi for &FakeDesks {
    type Handle = String;

    fn open_input(&self) -> Option<(String, String)> {
        let name = self.input.borrow().clone()?;
        self.done.borrow_mut().push(format!("open {name}"));
        Some((name.clone(), name))
    }

    fn set_thread(&self, desk: &String) -> bool {
        self.done.borrow_mut().push(format!("switch to {desk}"));
        !self.refuse
    }

    fn close(&self, desk: String) {
        self.done.borrow_mut().push(format!("close {desk}"));
    }
}

/// The hook thread's hooks, recorded in the desks' log.
struct Recorded<'a>(&'a FakeDesks);

impl ThreadHooks for Recorded<'_> {
    fn remove(&mut self) {
        self.0.done.borrow_mut().push("remove hooks".into());
    }

    fn install(&mut self) {
        self.0.done.borrow_mut().push("install hooks".into());
    }
}

fn hooks(desks: &FakeDesks) -> Recorded<'_> {
    Recorded(desks)
}

#[test]
fn desktops_are_told_apart_by_name() {
    assert_eq!(InputDesk::of_name("Default"), InputDesk::Default);
    assert_eq!(InputDesk::of_name("Winlogon"), InputDesk::Winlogon);
    assert_eq!(InputDesk::of_name("winlogon"), InputDesk::Winlogon);
    assert_eq!(InputDesk::of_name("Screen-saver"), InputDesk::Other);
    assert_eq!(InputDesk::Winlogon.diag(), DiagDesktop::Winlogon);
    assert_eq!(InputDesk::Other.diag(), DiagDesktop::Other);
    assert_eq!(InputDesk::Default.diag(), DiagDesktop::Default);
}

#[test]
fn what_is_published_reads_back() {
    let published = Published::default();
    for desk in [InputDesk::Other, InputDesk::Winlogon, InputDesk::Default] {
        published.switched(desk);
        assert_eq!(published.followed(), desk);
    }
    assert_eq!(published.generation(), 3);
}

#[test]
fn while_the_input_stays_on_this_threads_desktop_nothing_happens() {
    let desks = FakeDesks::on("Default");
    let published = Published::default();
    let mut follower = Follower::new(&desks, &published);
    assert_eq!(follower.check(&mut hooks(&desks)), None);
    assert_eq!(desks.done(), ["open Default", "close Default"]);
    assert_eq!(published.followed(), InputDesk::Default);
    assert_eq!(published.generation(), 0);
}

#[test]
fn a_new_input_desktop_is_followed_with_the_hooks_moved_and_injection_told() {
    let desks = FakeDesks::on("Winlogon");
    let published = Published::default();
    let mut follower = Follower::new(&desks, &published);
    let lines = heard_during(|| {
        assert_eq!(
            follower.check(&mut hooks(&desks)),
            Some(InputDesk::Winlogon)
        );
    });
    // Hooks bind a thread to its desktop: off first, back on after.
    assert_eq!(
        desks.done(),
        [
            "open Winlogon",
            "remove hooks",
            "switch to Winlogon",
            "install hooks"
        ]
    );
    assert_eq!(published.followed(), InputDesk::Winlogon);
    assert_eq!(published.generation(), 1, "injection follows");
    let desktop = ("desktop", DiagValue::Str("winlogon".into()));
    assert!(
        lines
            .iter()
            .any(|(_, event, fields)| event == "capture.desktop" && fields.contains(&desktop)),
        "{lines:?}"
    );
    // Back to the user's own: the Winlogon handle is let go once left.
    desks.show(Some("Default"));
    assert_eq!(follower.check(&mut hooks(&desks)), Some(InputDesk::Default));
    assert_eq!(
        desks.done(),
        [
            "open Default",
            "remove hooks",
            "switch to Default",
            "install hooks",
            "close Winlogon"
        ]
    );
    assert_eq!(published.followed(), InputDesk::Default);
    assert_eq!(published.generation(), 2);
}

#[test]
fn a_desktop_this_process_may_not_open_is_not_followed() {
    let desks = FakeDesks::default();
    let published = Published::default();
    let mut follower = Follower::new(&desks, &published);
    assert_eq!(follower.check(&mut hooks(&desks)), None);
    assert!(desks.done().is_empty());
    assert_eq!(published.followed(), InputDesk::Default);
}

#[test]
fn a_refused_switch_keeps_the_hooks_where_they_were() {
    let desks = FakeDesks {
        refuse: true,
        ..FakeDesks::on("Winlogon")
    };
    let published = Published::default();
    let mut follower = Follower::new(&desks, &published);
    assert_eq!(follower.check(&mut hooks(&desks)), None);
    assert_eq!(
        desks.done(),
        [
            "open Winlogon",
            "remove hooks",
            "switch to Winlogon",
            "install hooks",
            "close Winlogon"
        ]
    );
    assert_eq!(published.followed(), InputDesk::Default);
    assert_eq!(published.generation(), 0);
}

#[test]
fn injection_follows_once_per_switch() {
    let desks = FakeDesks::on("Winlogon");
    let published = Published::default();
    let mut seen = 0;
    // Nothing switched yet: nothing to do.
    assert!(!follow_for_injection(&&desks, &published, &mut seen));
    assert!(desks.done().is_empty());
    published.switched(InputDesk::Winlogon);
    assert!(follow_for_injection(&&desks, &published, &mut seen));
    assert_eq!(desks.done(), ["open Winlogon", "switch to Winlogon"]);
    assert!(!follow_for_injection(&&desks, &published, &mut seen));
    assert!(desks.done().is_empty());
}

#[test]
fn the_real_input_desktop_is_this_users_own_and_can_be_switched_to() {
    let real = System;
    let (desk, name) = real.open_input().expect("the input desktop opens");
    assert_eq!(InputDesk::of_name(&name), InputDesk::Default);
    // Switching a thread with no windows or hooks to the desktop it is on.
    let switched = std::thread::scope(|s| s.spawn(|| real.set_thread(&desk)).join().unwrap());
    assert!(switched);
    real.close(desk);
}
