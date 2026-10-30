//! Story 3 scenario 3: re-dial after this computer's own address changed,
//! and the guarantees a replaced connection must keep.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use mouser_platform::{InterfaceKind, LocalInput};
use mouser_proto::control::ConnectionState;
use mouser_proto::wire::ControlMessage;
use mouser_testkit::Injected;

use super::DIAL_ATTEMPT;
use crate::engine::Phase;
use crate::testing::*;

/// The accepting side's address changes.
fn change_address_of(node: &TestNode) {
    node.sim.as_ref().unwrap().add_interface(interface(
        "en9",
        InterfaceKind::PhysicalLan,
        [192, 168, 9, 9],
    ));
}

/// The operating system moves the computer to a new address, then tells
/// Mouser; the accepting side dials its peer again and keeps the session.
#[tokio::test]
async fn the_accepting_side_dials_the_peer_again_and_resumes_the_session() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, acceptor) = roles(&nodes);
    let (d, a) = (&nodes[dialler], &nodes[acceptor]);
    let (old_link, dialled, _) = link_of(a, d).unwrap();
    assert!(!dialled, "the acceptor accepted");

    // The address changes first (the endpoint moves, the network follows) ...
    let moved = a.engine.ctx.transport.rebind();
    a.sim
        .as_ref()
        .unwrap()
        .change_address(SocketAddr::from(([127, 0, 0, 1], moved.port())));
    // ... and then the platform reports it.
    let started = Instant::now();
    a.engine.ctx.redial_accepted();

    // The acceptor now holds a connection it dialled itself: new Hello and
    // DeskAuth, and the session continues over it.
    eventually("the acceptor's connection to be replaced", || async {
        link_of(a, d).is_some_and(|(link, dialled, phase)| {
            link != old_link && dialled && phase == Phase::Authed
        })
    })
    .await;
    wait_connection(a, d, ConnectionState::Connected).await;
    wait_connection(d, a, ConnectionState::Connected).await;
    assert!(started.elapsed() < Duration::from_secs(5));
    // The peer, which had dialled, now holds an accepted connection.
    assert!(!link_of(d, a).unwrap().1);
    eventually("nothing left to dial", || async {
        a.engine.ctx.core.lock().unwrap().redial.is_empty()
    })
    .await;
    crossing_works_both_ways(&nodes).await;
}

/// The whole path, as the platform sees it: an interface appears, and
/// whichever way the connection is re-established, the desk is whole again
/// within the five seconds of SC-008 and crossing works both ways.
#[tokio::test]
async fn an_address_change_on_the_accepting_side_heals_without_any_action() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, acceptor) = roles(&nodes);
    let (d, a) = (&nodes[dialler], &nodes[acceptor]);
    let old_link = link_of(a, d).unwrap().0;
    let started = Instant::now();
    change_address_of(a);
    eventually("a new connection", || async {
        link_of(a, d).is_some_and(|(link, _, phase)| link != old_link && phase == Phase::Authed)
            && link_of(d, a).is_some_and(|(_, _, phase)| phase == Phase::Authed)
    })
    .await;
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "{:?}",
        started.elapsed()
    );
    crossing_works_both_ways(&nodes).await;
}

async fn crossing_works_both_ways(nodes: &[TestNode]) {
    for from in [0, 1] {
        let to = 1 - from;
        nodes[to].platform.take_injected();
        push_toward(nodes, from);
        injected_where(&nodes[to].platform, "the pointer arriving", |i| {
            matches!(i, Injected::MotionAbs { .. })
        })
        .await;
        let warps = nodes[from].platform.warps().len();
        push_back(nodes, from);
        eventually("the pointer coming back", || async {
            nodes[from].platform.warps().len() > warps
        })
        .await;
    }
}

#[tokio::test]
async fn a_peer_that_cannot_be_found_is_left_alone() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, acceptor) = roles(&nodes);
    let (d, a) = (&nodes[dialler], &nodes[acceptor]);
    let link = link_of(a, d).unwrap().0;
    lab.registry.hide(d.engine.computer_id());
    change_address_of(a);
    eventually("the endpoint to move", || async {
        a.discovery.announcements().len() > 1
    })
    .await;
    settle().await;
    assert_eq!(
        link_of(a, d).unwrap().0,
        link,
        "no re-dial without an address"
    );
    assert!(a.engine.ctx.core.lock().unwrap().dialing.is_empty());
}

/// Regression (constitution VI): a connection replaced by a new one to the
/// same computer must not leave the old session's keys held down.
#[tokio::test]
async fn replacing_a_connection_releases_what_the_old_session_held() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    a.platform.send_input(LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    });
    injected_where(&b.platform, "the key", |i| {
        *i == Injected::Key {
            usage: 0x0007_0004,
            down: true,
        }
    })
    .await;

    // Whichever computer accepted the connection dials again.
    let (_, acceptor) = roles(&nodes);
    change_address_of(&nodes[acceptor]);
    injected_where(&b.platform, "the key to be released", |i| {
        *i == Injected::Key {
            usage: 0x0007_0004,
            down: false,
        }
    })
    .await;
    eventually("the mouse to come back to Alpha", || async {
        !a.platform.is_grabbed()
    })
    .await;
    wait_connection(a, b, ConnectionState::Connected).await;
}

/// Regression: a message still queued on a link that was replaced must not
/// be applied as if the new connection had sent it.
#[tokio::test]
async fn messages_from_a_replaced_link_are_ignored() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    send_to(
        a,
        b,
        ControlMessage::Enter {
            display_id: mouser_proto::model::DisplayId {
                computer: b.engine.computer_id(),
                key: "virtual-0".into(),
            },
            x: 1.0,
            y: 1.0,
            seq_base: 1,
            modifiers_down: Default::default(),
        },
    );
    injected_where(&b.platform, "the pointer", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    // Beta now holds a newer link to Alpha than the one this reader serves.
    let id = a.engine.computer_id();
    let current = b.engine.ctx.core.lock().unwrap().peers[&id].link_id;
    assert!(b.engine.ctx.is_current(id, current));
    b.engine
        .ctx
        .core
        .lock()
        .unwrap()
        .peers
        .get_mut(&id)
        .unwrap()
        .link_id += 100;
    assert!(!b.engine.ctx.is_current(id, current));
    send_to(
        a,
        b,
        ControlMessage::Key {
            seq: 2,
            usage: 0x0007_0004,
            down: true,
            modifiers: Default::default(),
        },
    );
    settle().await;
    assert!(
        !b.platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::Key { .. })),
        "{:?}",
        b.platform.injected()
    );
}

#[tokio::test]
async fn a_computer_is_dialled_at_every_address_it_has() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let dead = SocketAddr::from(([127, 0, 0, 1], free_port()));
    let live = SocketAddr::from(([127, 0, 0, 1], b.engine.local_addr().port()));
    let answered = a
        .engine
        .ctx
        .dial_any(&[dead, live], b.engine.computer_id())
        .await;
    assert_eq!(answered.unwrap().1.computer_id, b.engine.computer_id());
}

#[tokio::test]
async fn an_attempt_to_reach_an_unreachable_computer_ends_within_the_limit() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let dead = SocketAddr::from(([127, 0, 0, 1], free_port()));
    let started = Instant::now();
    assert!(
        a.engine
            .ctx
            .dial_any(&[dead], b.engine.computer_id())
            .await
            .is_none()
    );
    assert!(started.elapsed() < DIAL_ATTEMPT + Duration::from_millis(500));
}

/// Regression: a reader task that panics must show its own panic, not an
/// abort from the link guard's cleanup finding the engine state poisoned.
#[tokio::test]
async fn a_panic_with_the_engine_state_poisoned_is_not_turned_into_an_abort() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (link, id) = (link_of(a, b).unwrap().0, b.engine.computer_id());
    let ctx = a.engine.ctx.clone();
    // Something panics holding the state.
    let poisoned = ctx.clone();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _core = poisoned.core.lock().unwrap();
        panic!("first panic");
    }));
    assert!(ctx.core.is_poisoned());
    // Then a link's task panics: the guard dropped while unwinding must not
    // touch the poisoned state (which would abort the whole process).
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = super::LinkGuard {
            ctx: ctx.clone(),
            peer: id,
            link_id: link,
        };
        panic!("the original panic");
    }));
    let payload = result.unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"the original panic"));
}

#[tokio::test]
async fn an_attempt_answered_by_the_wrong_computer_fails_at_once() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let live = SocketAddr::from(([127, 0, 0, 1], b.engine.local_addr().port()));
    let started = Instant::now();
    let stranger = mouser_proto::ids::ComputerId([9; 16]);
    assert!(a.engine.ctx.dial_any(&[live], stranger).await.is_none());
    assert!(started.elapsed() < DIAL_ATTEMPT);
}

/// A dial attempt that ends any way but by finishing (here it is cut short,
/// as a panic or the engine stopping would) must not leave the computer in
/// `dialing`, or it is never dialled again.
#[tokio::test]
async fn a_dial_attempt_cut_short_leaves_the_computer_to_be_dialled_again() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let peer = b.engine.computer_id();
    let found = crate::discovery::Found {
        computer: peer,
        display_name: "Beta".into(),
        protocol: mouser_proto::wire::PROTOCOL_VERSION,
        desk: None,
        addrs: vec![SocketAddr::from(([127, 0, 0, 1], free_port()))],
    };
    a.engine.ctx.core.lock().unwrap().dialing.insert(peer);
    let attempt = tokio::spawn(a.engine.ctx.clone().dial_found(found));
    settle().await;
    assert!(a.engine.ctx.core.lock().unwrap().dialing.contains(&peer));
    attempt.abort();
    let _ = attempt.await;
    assert!(!a.engine.ctx.core.lock().unwrap().dialing.contains(&peer));
}

/// Regression (T537): a re-dial that has succeeded is done. Left pending
/// until the next tick, it outlived the connection it made: when the peer's
/// own address changed next and its re-dial replaced that connection, this
/// side, now holding an accepted connection again, dialled once more and
/// replaced the peer's, handing back a crossing just made over it.
#[tokio::test]
async fn a_re_dial_that_succeeded_leaves_nothing_to_dial() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, acceptor) = roles(&nodes);
    let (d, a) = (&nodes[dialler], &nodes[acceptor]);
    let peer = d.engine.computer_id();
    let found = a
        .engine
        .ctx
        .discovery
        .found()
        .into_iter()
        .find(|f| f.computer == peer)
        .unwrap();
    {
        let mut core = a.engine.ctx.core.lock().unwrap();
        core.redial.insert(peer);
        core.dialing.insert(peer);
    }
    a.engine.ctx.clone().dial_found(found).await;
    assert!(link_of(a, d).unwrap().1, "the acceptor dialled");
    assert!(!a.engine.ctx.core.lock().unwrap().redial.contains(&peer));
}

/// Both computers' addresses change, one right after the other: each
/// re-dials once, and the connection the second one dialled is kept.
#[tokio::test]
async fn addresses_changing_one_after_the_other_settle_on_the_last_re_dial() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, acceptor) = roles(&nodes);
    let (d, a) = (&nodes[dialler], &nodes[acceptor]);
    change_address_of(a);
    eventually("the acceptor's re-dial", || async {
        link_of(a, d).is_some_and(|(_, dialled, _)| dialled)
    })
    .await;
    // At once, the other computer's address changes: it now holds the
    // accepted connection and dials again.
    d.sim.as_ref().unwrap().add_interface(interface(
        "en7",
        InterfaceKind::PhysicalLan,
        [192, 168, 7, 7],
    ));
    eventually("the other computer's re-dial", || async {
        link_of(d, a).is_some_and(|(_, dialled, phase)| dialled && phase == Phase::Authed)
    })
    .await;
    let kept = link_of(d, a).unwrap().0;
    // Several ticks later it is still the connection in use.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert_eq!(link_of(d, a).unwrap().0, kept);
    assert!(link_of(d, a).unwrap().1);
}
