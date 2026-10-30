//! Story 3 scenarios 2 to 4: the network changes under a running desk.

use std::time::Duration;

use mouser_platform::InterfaceKind;
use mouser_proto::control::ConnectionState;
use mouser_testkit::Injected;

use crate::testing::*;

#[tokio::test]
async fn rebinding_keeps_the_configured_port_and_the_wildcard_address() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let before = node.engine.local_addr();
    assert!(before.ip().is_unspecified());
    let after = node.engine.ctx.transport.rebind();
    assert_eq!(after, before);
    assert_eq!(node.engine.local_addr(), before);
}

/// Regression: an address change used to move the endpoint to an OS-chosen
/// port, which broke join-by-address and firewall rules on `advanced.port`.
#[tokio::test]
async fn an_address_change_keeps_the_configured_port() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    for node in &nodes {
        let configured =
            u16::try_from(node.engine.ctx.core.lock().unwrap().config.advanced.port).unwrap();
        assert_eq!(node.engine.local_addr().port(), configured);
        let announced = node.discovery.announcements().len();
        node.sim.as_ref().unwrap().add_interface(interface(
            "en6",
            InterfaceKind::PhysicalLan,
            [192, 168, 6, 6],
        ));
        eventually("the change to be handled", || async {
            node.discovery.announcements().len() > announced
        })
        .await;
        assert_eq!(node.engine.local_addr().port(), configured);
        assert_eq!(
            node.discovery.announcements().last().unwrap().port,
            configured
        );
    }
    eventually("both sides connected", || async {
        connected(&nodes[0]).await == 1 && connected(&nodes[1]).await == 1
    })
    .await;
}

#[tokio::test]
async fn a_network_change_reclassifies_interfaces_and_announces_again() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let announced = a.discovery.announcements().len();
    let sim = a.sim.as_ref().unwrap();

    // A VPN starts.
    sim.add_interface(interface("utun3", InterfaceKind::Virtual, [10, 8, 0, 2]));
    eventually("the interfaces to be reclassified", || async {
        a.discovery
            .interface_changes()
            .last()
            .is_some_and(|list| list.iter().any(|i| i.name == "utun3"))
    })
    .await;
    // It was announced again, with the endpoint's current port.
    eventually("a new announcement", || async {
        a.discovery.announcements().len() > announced
    })
    .await;
    assert_eq!(
        a.discovery.announcements().last().unwrap().port,
        a.engine.local_addr().port()
    );

    // And it stops.
    assert!(sim.remove_interface("utun3"));
    eventually("the interfaces to be reclassified again", || async {
        a.discovery
            .interface_changes()
            .last()
            .is_some_and(|list| list.iter().all(|i| i.name != "utun3"))
    })
    .await;

    // Nobody had to do anything: the desk is as connected as before.
    assert_eq!(connected(a).await, 1);
    assert_eq!(connected(b).await, 1);
}

#[tokio::test]
async fn the_dialling_side_keeps_its_connection_through_an_address_change() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, acceptor) = roles(&nodes);
    let (d, a) = (&nodes[dialler], &nodes[acceptor]);
    let link = link_of(d, a).unwrap().0;
    let stable = d.engine.ctx.core.lock().unwrap().peers[&a.engine.computer_id()]
        .conn
        .stable_id();
    let announced = d.discovery.announcements().len();

    // The dialler moves from Wi-Fi to Ethernet.
    let sim = d.sim.as_ref().unwrap();
    sim.add_interface(interface(
        "en7",
        InterfaceKind::PhysicalLan,
        [192, 168, 7, 9],
    ));
    eventually("the endpoint to be rebound", || async {
        d.discovery.announcements().len() > announced
    })
    .await;

    // The same connection carries on: migrated, not re-established.
    wait_connection(a, d, ConnectionState::Connected).await;
    assert_eq!(link_of(d, a).unwrap().0, link, "no new link");
    assert_eq!(
        d.engine.ctx.core.lock().unwrap().peers[&a.engine.computer_id()]
            .conn
            .stable_id(),
        stable
    );
    // Crossing works afterwards.
    push_toward(&nodes, dialler);
    injected_where(&a.platform, "the pointer on the other computer", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
}

/// Push the pointer across from `from` until it arrives on the other
/// computer. Right after an address change the peer may redial and replace the
/// connection (`Ctx::adopt`), which hands a crossing already under way back to
/// the computer it left; the user then pushes again, and so does this.
async fn push_until_it_arrives(nodes: &[TestNode], from: usize) {
    let arrived = || {
        nodes[1 - from]
            .platform
            .injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { .. }))
    };
    let end = std::time::Instant::now() + Duration::from_secs(15);
    while !arrived() {
        assert!(
            std::time::Instant::now() < end,
            "timed out waiting for the pointer arriving"
        );
        push_toward(nodes, from);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn crossing_works_within_five_seconds_of_an_address_change() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    for (mover, other) in [(0, 1), (1, 0)] {
        let started = std::time::Instant::now();
        let announced = nodes[mover].discovery.announcements().len();
        nodes[mover].sim.as_ref().unwrap().add_interface(interface(
            "en8",
            InterfaceKind::PhysicalLan,
            [172, 16, 0, 9 + mover as u8],
        ));
        eventually("the endpoint to be rebound", || async {
            nodes[mover].discovery.announcements().len() > announced
        })
        .await;
        // A mover that had accepted the connection dials again, and the new
        // connection replaces the old one (`Ctx::adopt`), handing back a
        // crossing made on the old one, which is still connected until then.
        // So the crossings start once the mover holds a connection it
        // dialled and both sides have each other placed on it.
        eventually("the mover to hold a connection it dialled", || async {
            link_of(&nodes[mover], &nodes[other]).is_some_and(|(_, dialled, _)| dialled)
        })
        .await;
        ready(&nodes).await;
        // The pointer crosses in both directions.
        for from in [mover, other] {
            nodes[1 - from].platform.take_injected();
            push_until_it_arrives(&nodes, from).await;
            // Hand the pointer back for the next round.
            let returns = nodes[from].platform.warps().len();
            push_back(&nodes, from);
            eventually("the pointer coming back", || async {
                nodes[from].platform.warps().len() > returns
            })
            .await;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }
}

/// Regression: a VPN, virtual machine or container network starting, or a
/// wake that changes no address, must not move the endpoint or replace a
/// working connection (which would hand the mouse back mid-session).
#[tokio::test]
async fn changes_that_leave_the_physical_addresses_alone_leave_the_session_alone() {
    let lab = Lab::simulated();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, acceptor) = roles(&nodes);
    let (d, a) = (&nodes[dialler], &nodes[acceptor]);
    push_toward(&nodes, dialler);
    injected_where(&a.platform, "the pointer on the other computer", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    let (links, ports) = (
        (link_of(d, a), link_of(a, d)),
        (d.engine.local_addr(), a.engine.local_addr()),
    );
    for node in [d, a] {
        let sim = node.sim.as_ref().unwrap();
        let announced = node.discovery.announcements().len();
        sim.add_interface(interface("utun4", InterfaceKind::Virtual, [10, 9, 0, 2]));
        sim.add_interface(interface(
            "docker0",
            InterfaceKind::Virtual,
            [172, 17, 0, 1],
        ));
        node.platform.trigger_network_change();
        eventually("the change to be handled", || async {
            node.discovery.announcements().len() > announced
        })
        .await;
    }
    settle().await;
    assert_eq!((link_of(d, a), link_of(a, d)), links, "same connections");
    assert_eq!((d.engine.local_addr(), a.engine.local_addr()), ports);
    assert!(
        d.platform.is_grabbed(),
        "the mouse is still on the other computer"
    );
    assert_eq!(connected(d).await, 1);
}
