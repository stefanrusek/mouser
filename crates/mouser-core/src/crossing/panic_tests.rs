use std::time::{Duration, Instant};

use super::panic::*;

const ESC: u32 = 0x0007_0029;
const A: u32 = 0x0007_0004;

#[test]
fn key_names_map_to_hid_usages() {
    assert_eq!(usage_for_name("Escape"), Some(ESC));
    assert_eq!(usage_for_name("Space"), Some(0x0007_002C));
    assert_eq!(usage_for_name("Enter"), Some(0x0007_0028));
    assert_eq!(usage_for_name("Tab"), Some(0x0007_002B));
    assert_eq!(usage_for_name("Backspace"), Some(0x0007_002A));
    assert_eq!(usage_for_name("A"), Some(A));
    assert_eq!(usage_for_name("Z"), Some(0x0007_001D));
    assert_eq!(usage_for_name("1"), Some(0x0007_001E));
    assert_eq!(usage_for_name("9"), Some(0x0007_0026));
    assert_eq!(usage_for_name("0"), Some(0x0007_0027));
    assert_eq!(usage_for_name("F1"), Some(0x0007_003A));
    assert_eq!(usage_for_name("F12"), Some(0x0007_0045));
    for bad in ["", "a", "F0", "F13", "Fx", "Esc", "AB", "-"] {
        assert_eq!(usage_for_name(bad), None, "{bad}");
    }
}

#[test]
fn the_default_spec_is_escape_twice() {
    assert_eq!(
        PanicSpec::parse("Escape*2"),
        Ok(PanicSpec {
            usage: ESC,
            presses: 2
        })
    );
    assert!(
        PanicSpec::parse("Escape")
            .unwrap_err()
            .contains("<key>*<count>")
    );
    assert!(
        PanicSpec::parse("Nope*2")
            .unwrap_err()
            .contains("known key")
    );
    assert!(
        PanicSpec::parse("Escape*0")
            .unwrap_err()
            .contains("presses")
    );
    assert!(
        PanicSpec::parse("Escape*x")
            .unwrap_err()
            .contains("presses")
    );
}

fn detector(presses: u32) -> PanicDetector {
    PanicDetector::new(PanicSpec {
        usage: ESC,
        presses,
    })
}

#[test]
fn two_quick_presses_fire_and_start_over() {
    let t = Instant::now();
    let mut d = detector(2);
    assert!(!d.pressed(ESC, t));
    assert!(d.pressed(ESC, t + Duration::from_millis(200)));
    assert!(!d.pressed(ESC, t + Duration::from_millis(300)));
    assert!(d.pressed(ESC, t + Duration::from_millis(400)));
}

#[test]
fn slow_presses_and_other_keys_do_not_add_up() {
    let t = Instant::now();
    let mut d = detector(2);
    assert!(!d.pressed(ESC, t));
    assert!(!d.pressed(ESC, t + WINDOW + Duration::from_millis(1)));
    assert!(!d.pressed(A, t + WINDOW));
    assert!(!d.pressed(ESC, t + WINDOW));
    d.reset();
    assert!(!d.pressed(ESC, t + WINDOW));
}

#[test]
fn a_single_press_gesture_fires_at_once() {
    assert!(detector(1).pressed(ESC, Instant::now()));
}

mod on_the_wire {
    use mouser_platform::LocalInput;
    use mouser_testkit::Injected;

    use crate::testing::*;

    const ESC: u32 = 0x0007_0029;
    const A: u32 = 0x0007_0004;

    fn tap(node: &TestNode, usage: u32) {
        node.platform
            .send_input(LocalInput::Key { usage, down: true });
        node.platform
            .send_input(LocalInput::Key { usage, down: false });
    }

    #[tokio::test]
    async fn two_escapes_while_remote_release_everything_and_bring_control_home() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        push_right(a);
        eventually("the grab", || async { a.platform.is_grabbed() }).await;
        a.platform.send_input(LocalInput::Key {
            usage: A,
            down: true,
        });
        injected_where(&b.platform, "the held key", |i| {
            *i == Injected::Key {
                usage: A,
                down: true,
            }
        })
        .await;
        tap(a, ESC);
        tap(a, ESC);
        eventually("control to come home", || async {
            !a.platform.is_grabbed()
        })
        .await;
        injected_where(&b.platform, "the release", |i| {
            *i == Injected::Key {
                usage: A,
                down: false,
            }
        })
        .await;
        // The pointer is back where it left from.
        assert_eq!(a.platform.warps(), [(1919.0, 540.0)]);
        // The first Escape reached Beta; the second was the panic, not input.
        let escapes = b
            .platform
            .injected()
            .iter()
            .filter(|i| {
                matches!(
                    i,
                    Injected::Key {
                        usage: ESC,
                        down: true
                    }
                )
            })
            .count();
        assert_eq!(escapes, 1);
        // Beta takes no more input from Alpha until the next Enter.
        let before = b.platform.injected().len();
        settle().await;
        assert_eq!(b.platform.injected().len(), before);
    }

    #[tokio::test]
    async fn one_escape_is_just_a_key() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        push_right(a);
        eventually("the grab", || async { a.platform.is_grabbed() }).await;
        tap(a, ESC);
        injected_where(&b.platform, "the escape", |i| {
            *i == Injected::Key {
                usage: ESC,
                down: true,
            }
        })
        .await;
        assert!(a.platform.is_grabbed());
    }

    /// Alpha's shared mouse drives Beta, holding A there.
    async fn alpha_drives_beta(a: &TestNode, b: &TestNode) {
        push_right(a);
        eventually("the grab", || async { a.platform.is_grabbed() }).await;
        a.platform.send_input(LocalInput::Key {
            usage: A,
            down: true,
        });
        injected_where(&b.platform, "the held key", |i| {
            *i == Injected::Key {
                usage: A,
                down: true,
            }
        })
        .await;
    }

    #[tokio::test]
    async fn two_escapes_on_the_controlled_computer_bring_the_controller_home() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        alpha_drives_beta(a, b).await;
        tap(b, ESC);
        tap(b, ESC);
        eventually("Alpha to stop forwarding", || async {
            !a.platform.is_grabbed()
        })
        .await;
        injected_where(&b.platform, "the release", |i| {
            *i == Injected::Key {
                usage: A,
                down: false,
            }
        })
        .await;
        assert!(!b.platform.is_grabbed());
    }

    #[tokio::test]
    async fn two_escapes_on_an_uninvolved_computer_bring_the_controller_home() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
        let (a, b, c) = (&nodes[0], &nodes[1], &nodes[2]);
        alpha_drives_beta(a, b).await;
        tap(c, ESC);
        tap(c, ESC);
        eventually("Alpha to stop forwarding", || async {
            !a.platform.is_grabbed()
        })
        .await;
        injected_where(&b.platform, "the release", |i| {
            *i == Injected::Key {
                usage: A,
                down: false,
            }
        })
        .await;
    }

    #[tokio::test]
    async fn the_controller_panicking_tells_every_desk_mate() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
        let (a, b, c) = (&nodes[0], &nodes[1], &nodes[2]);
        alpha_drives_beta(a, b).await;
        tap(a, ESC);
        tap(a, ESC);
        eventually("control to come home", || async {
            !a.platform.is_grabbed()
        })
        .await;
        // Gamma, which nothing was driving, changes nothing.
        settle().await;
        assert!(!c.platform.is_grabbed());
        assert!(
            c.platform.injected().is_empty(),
            "{:?}",
            c.platform.injected()
        );
    }

    #[tokio::test]
    async fn the_controlled_computer_crossing_back_with_its_own_mouse_takes_over() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        alpha_drives_beta(a, b).await;
        push_left(b);
        eventually("Beta to drive Alpha", || async { b.platform.is_grabbed() }).await;
        eventually("Alpha to stop forwarding", || async {
            !a.platform.is_grabbed()
        })
        .await;
        // What Alpha held on Beta is let go when Beta's own devices take over.
        injected_where(&b.platform, "the release", |i| {
            *i == Injected::Key {
                usage: A,
                down: false,
            }
        })
        .await;
    }
}

mod reconfigured {
    use mouser_platform::LocalInput;
    use mouser_proto::control::{Empty, Method};
    use mouser_testkit::Injected;
    use serde_json::json;

    use crate::testing::*;

    const ESC: u32 = 0x0007_0029;
    const F5: u32 = 0x0007_003E;

    fn tap(node: &TestNode, usage: u32) {
        for down in [true, false] {
            node.platform.send_input(LocalInput::Key { usage, down });
        }
    }

    #[tokio::test]
    async fn a_changed_panic_key_works_at_once_and_the_previous_one_no_longer_does() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let _: Empty = call(
            a,
            Method::ConfigSet,
            json!({"key": "hotkeys.panic", "value": "F5*3"}),
        )
        .await;
        push_right(a);
        eventually("the grab", || async { a.platform.is_grabbed() }).await;
        // The previous gesture is now only keys, delivered to Beta.
        tap(a, ESC);
        tap(a, ESC);
        let escapes = || {
            b.platform
                .injected()
                .iter()
                .filter(|i| {
                    matches!(
                        i,
                        Injected::Key {
                            usage: ESC,
                            down: true
                        }
                    )
                })
                .count()
        };
        eventually("both escapes to arrive", || async { escapes() == 2 }).await;
        assert!(a.platform.is_grabbed());
        // Two presses of the new key are not enough; the third is the panic.
        tap(a, F5);
        tap(a, F5);
        eventually("two F5s to arrive", || async {
            b.platform
                .injected()
                .iter()
                .filter(|i| {
                    matches!(
                        i,
                        Injected::Key {
                            usage: F5,
                            down: true
                        }
                    )
                })
                .count()
                == 2
        })
        .await;
        assert!(a.platform.is_grabbed());
        tap(a, F5);
        eventually("control to come home", || async {
            !a.platform.is_grabbed()
        })
        .await;
    }
}
