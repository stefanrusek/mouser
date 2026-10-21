//! Story 3 scenario 1 and SC-002: keystrokes arrive in order, none missing,
//! however the network treats the packets.

use std::time::Duration;

use mouser_platform::LocalInput;
use mouser_proto::control::Method;
use mouser_testkit::Injected;
use serde_json::json;

use super::ordering::Sequencer;
use crate::testing::*;

#[test]
fn the_sequencer_admits_only_newer_numbers() {
    let mut s = Sequencer::after(5);
    assert!(!s.admit(4), "older than the base");
    assert!(!s.admit(5), "the base itself was the Enter");
    assert!(s.admit(6));
    assert!(!s.admit(6), "a repeat");
    assert!(
        s.admit(9),
        "gaps are fine: other messages took the numbers between"
    );
    assert!(!s.admit(8), "overtaken");
}

/// 10 000 keystrokes typed on another computer while the network alternates
/// between instant and slow, so datagrams and stream packets overtake each
/// other: every key arrives, in the order typed, and the pointer never goes
/// back to an older position.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ten_thousand_keystrokes_arrive_in_order_over_a_delaying_network() {
    const KEYS: u32 = 10_000;
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    b.platform.take_injected();

    let slow = b.sim.clone().expect("simulated");
    let typist = a.platform.clone();
    let typing = tokio::spawn(async move {
        for n in 0..KEYS {
            // Each stage of 250 keys is slow or instant in turn.
            if n % 250 == 0 {
                slow.set_delay(if (n / 250) % 2 == 0 {
                    Duration::from_millis(30)
                } else {
                    Duration::ZERO
                });
            }
            let usage = 0x0007_0004 + n % 26;
            typist.send_input(LocalInput::Key { usage, down: true });
            typist.send_input(LocalInput::Motion {
                dx: 0.02,
                dy: 0.0,
                abs: (960.0, 540.0),
            });
            typist.send_input(LocalInput::Key { usage, down: false });
            if n % 100 == 0 {
                tokio::task::yield_now().await;
            }
        }
        slow.set_delay(Duration::ZERO);
    });
    typing.await.unwrap();
    let expected: Vec<Injected> = (0..KEYS)
        .flat_map(|n| {
            let usage = 0x0007_0004 + n % 26;
            [
                Injected::Key { usage, down: true },
                Injected::Key { usage, down: false },
            ]
        })
        .collect();
    eventually("every keystroke to arrive", || async {
        keys_of(&b.platform.injected()).len() >= expected.len()
    })
    .await;
    // Let anything slow still in flight land before judging.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let seen = b.platform.injected();
    assert_eq!(keys_of(&seen), expected, "keys in order, none missing");
    let xs: Vec<f64> = seen
        .iter()
        .filter_map(|i| match i {
            Injected::MotionAbs { x, .. } => Some(*x),
            _ => None,
        })
        .collect();
    assert!(
        xs.windows(2).all(|w| w[0] <= w[1]),
        "the pointer never goes back to an older position"
    );
    // The session did not end.
    assert_eq!(connected(a).await, 1);
    assert_eq!(connected(b).await, 1);
    let _: mouser_proto::control::Status = call(a, Method::Status, json!({})).await;
}

fn keys_of(injected: &[Injected]) -> Vec<Injected> {
    injected
        .iter()
        .filter(|i| matches!(i, Injected::Key { .. }))
        .cloned()
        .collect()
}
