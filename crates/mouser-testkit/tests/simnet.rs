//! SimNet capabilities: delay, black-hole, re-address, interfaces, sleep/wake.

use mouser_platform::{InterfaceKind, NetInterface, Platform};
use mouser_testkit::{SimNet, SimNode, VirtualPlatform};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::time::timeout;

struct App {
    node: SimNode,
    socket: UdpSocket,
}

async fn app(net: &SimNet, name: &str) -> App {
    let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let node = net.add_node(name, socket.local_addr().unwrap());
    App { node, socket }
}

async fn recv(socket: &UdpSocket, wait: Duration) -> Option<(Vec<u8>, SocketAddr)> {
    let mut buf = [0u8; 1500];
    let (n, from) = timeout(wait, socket.recv_from(&mut buf))
        .await
        .ok()?
        .unwrap();
    Some((buf[..n].to_vec(), from))
}

const WAIT: Duration = Duration::from_millis(500);
const SHORT: Duration = Duration::from_millis(150);

#[tokio::test]
async fn relays_both_directions() {
    let net = SimNet::new();
    let (a, b) = (app(&net, "a").await, app(&net, "b").await);
    assert_eq!(a.node.name(), "a");
    assert_eq!(a.node.addr(), a.socket.local_addr().unwrap());
    let to_b = a.node.route_to(&b.node);
    assert_eq!(a.node.route_to(&b.node), to_b, "route is stable");

    a.socket.send_to(b"ping", to_b).await.unwrap();
    let (data, relay_src) = recv(&b.socket, WAIT).await.unwrap();
    assert_eq!(data, b"ping");
    assert_ne!(
        relay_src,
        a.socket.local_addr().unwrap(),
        "peer sees the relay"
    );

    b.socket.send_to(b"pong", relay_src).await.unwrap();
    let (data, from) = recv(&a.socket, WAIT).await.unwrap();
    assert_eq!(data, b"pong");
    assert_eq!(from, to_b);
}

#[tokio::test]
async fn delay_slows_packets_and_applies_both_ways() {
    let net = SimNet::new();
    let (a, b) = (app(&net, "a").await, app(&net, "b").await);
    let to_b = a.node.route_to(&b.node);
    b.node.set_delay(Duration::from_millis(120));

    let start = Instant::now();
    a.socket.send_to(b"x", to_b).await.unwrap();
    let (_, relay_src) = recv(&b.socket, WAIT).await.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(120));

    let start = Instant::now();
    b.socket.send_to(b"y", relay_src).await.unwrap();
    recv(&a.socket, WAIT).await.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(120));
}

#[tokio::test]
async fn blackhole_drops_both_directions_and_recovers() {
    let net = SimNet::new();
    let (a, b) = (app(&net, "a").await, app(&net, "b").await);
    let to_b = a.node.route_to(&b.node);
    a.socket.send_to(b"1", to_b).await.unwrap();
    let (_, relay_src) = recv(&b.socket, WAIT).await.unwrap();

    b.node.set_blackhole(true);
    a.socket.send_to(b"2", to_b).await.unwrap();
    b.socket.send_to(b"3", relay_src).await.unwrap();
    assert!(recv(&b.socket, SHORT).await.is_none());
    assert!(recv(&a.socket, SHORT).await.is_none());
    assert_eq!(net.dropped_packets(), 2);

    b.node.set_blackhole(false);
    a.socket.send_to(b"4", to_b).await.unwrap();
    assert_eq!(recv(&b.socket, WAIT).await.unwrap().0, b"4");
}

#[tokio::test]
async fn blackhole_during_flight_drops_the_packet() {
    let net = SimNet::new();
    let (a, b) = (app(&net, "a").await, app(&net, "b").await);
    let to_b = a.node.route_to(&b.node);
    b.node.set_delay(Duration::from_millis(200));
    a.socket.send_to(b"late", to_b).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    b.node.set_blackhole(true);
    assert!(recv(&b.socket, Duration::from_millis(400)).await.is_none());
}

#[tokio::test]
async fn address_change_shows_peers_a_new_source_and_keeps_routes() {
    let net = SimNet::new();
    let (a, b) = (app(&net, "a").await, app(&net, "b").await);
    let to_b = a.node.route_to(&b.node);
    let to_a = b.node.route_to(&a.node);

    // b dials a; a sees the relay's address for b.
    b.socket.send_to(b"hello", to_a).await.unwrap();
    let (_, b_seen_by_a) = recv(&a.socket, WAIT).await.unwrap();

    // b moves to a new socket (as after Endpoint::rebind).
    let moved = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    b.node.change_address(moved.local_addr().unwrap());
    assert_eq!(b.node.addr(), moved.local_addr().unwrap());
    assert_eq!(b.node.route_to(&a.node), to_a, "dial address unchanged");

    moved.send_to(b"moved", to_a).await.unwrap();
    let (data, b_seen_now) = recv(&a.socket, WAIT).await.unwrap();
    assert_eq!(data, b"moved");
    assert_ne!(b_seen_now, b_seen_by_a, "peer sees a new source address");

    // Replies to the new source reach the new socket; the old source is dead.
    a.socket.send_to(b"reply", b_seen_now).await.unwrap();
    assert_eq!(recv(&moved, WAIT).await.unwrap().0, b"reply");
    a.socket.send_to(b"stale", b_seen_by_a).await.unwrap();
    assert!(recv(&moved, SHORT).await.is_none());
    assert!(recv(&b.socket, SHORT).await.is_none());

    // Traffic toward b's dial address now lands on the new socket.
    a.socket.send_to(b"to-new", to_b).await.unwrap();
    assert_eq!(recv(&moved, WAIT).await.unwrap().0, b"to-new");
}

fn iface(name: &str, kind: InterfaceKind) -> NetInterface {
    NetInterface {
        name: name.into(),
        kind,
        addrs: vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))],
    }
}

#[tokio::test]
async fn interfaces_added_and_removed_reach_the_bound_platform() {
    let net = SimNet::new();
    let a = app(&net, "a").await;
    let vp = VirtualPlatform::new();
    a.node.bind_platform(&vp);
    assert_eq!(a.node.interfaces().len(), 2);
    let (tx, rx) = channel();
    let _watch = vp.watch_network(tx).unwrap();

    a.node.add_interface(iface("utun3", InterfaceKind::Virtual));
    assert!(rx.try_recv().is_ok());
    assert!(vp.interfaces().iter().any(|i| i.name == "utun3"));

    a.node.add_interface(iface("utun3", InterfaceKind::Virtual));
    assert_eq!(
        vp.interfaces().iter().filter(|i| i.name == "utun3").count(),
        1
    );
    rx.try_iter().for_each(drop);

    assert!(a.node.remove_interface("utun3"));
    assert!(rx.try_recv().is_ok());
    assert!(vp.interfaces().iter().all(|i| i.name != "utun3"));
    assert!(!a.node.remove_interface("utun3"));
}

#[tokio::test]
async fn interfaces_without_a_platform_are_just_recorded() {
    let net = SimNet::default();
    let a = app(&net, "a").await;
    a.node
        .add_interface(iface("eth1", InterfaceKind::PhysicalLan));
    assert_eq!(a.node.interfaces().len(), 1);
}

#[tokio::test]
async fn sleeping_node_loses_traffic_and_wake_signals_a_network_change() {
    let net = SimNet::new();
    let (a, b) = (app(&net, "a").await, app(&net, "b").await);
    let vp = VirtualPlatform::new();
    b.node.bind_platform(&vp);
    let (tx, rx) = channel();
    let _watch = vp.watch_network(tx).unwrap();
    let to_b = a.node.route_to(&b.node);

    b.node.sleep();
    assert!(b.node.is_asleep());
    a.socket.send_to(b"zzz", to_b).await.unwrap();
    assert!(recv(&b.socket, SHORT).await.is_none());
    assert!(rx.try_recv().is_err(), "no signal while asleep");

    b.node.wake();
    assert!(!b.node.is_asleep());
    assert!(rx.try_recv().is_ok(), "wake fires network watchers");
    a.socket.send_to(b"up", to_b).await.unwrap();
    assert_eq!(recv(&b.socket, WAIT).await.unwrap().0, b"up");
}

#[tokio::test]
async fn waking_a_node_without_a_platform_is_fine() {
    let net = SimNet::new();
    let a = app(&net, "a").await;
    a.node.sleep();
    a.node.wake();
    assert!(!a.node.is_asleep());
}

/// On Windows a socket that sent to a closed loopback port gets a connection
/// reset from its next receive. That must not end the relay direction.
#[tokio::test]
async fn relay_survives_a_send_to_a_closed_port() {
    let net = SimNet::new();
    let (a, b) = (app(&net, "a").await, app(&net, "b").await);
    let to_b = a.node.route_to(&b.node);

    a.socket.send_to(b"first", to_b).await.unwrap();
    let (_, relay_src) = recv(&b.socket, WAIT).await.unwrap();

    // b goes away; the relay's send to its closed port provokes the reset.
    let b_addr = b.node.addr();
    drop(b.socket);
    a.socket.send_to(b"into the void", to_b).await.unwrap();
    // Wait for a datagram that only arrives after the relay's tasks have run:
    // it goes through another relay link, so this task really parks, the I/O
    // driver wakes the relay task holding "into the void" first (its socket
    // became readable first), and that task sends while b is gone.
    let d = app(&net, "d").await;
    let to_d = a.node.route_to(&d.node);
    a.socket.send_to(b"sync", to_d).await.unwrap();
    recv(&d.socket, WAIT).await.unwrap();

    // b comes back on the same address; both directions still work.
    let b_socket = UdpSocket::bind(b_addr).await.unwrap();
    a.socket.send_to(b"again", to_b).await.unwrap();
    let (data, _) = recv(&b_socket, WAIT).await.unwrap();
    assert_eq!(data, b"again");
    b_socket.send_to(b"reply", relay_src).await.unwrap();
    let (data, from) = recv(&a.socket, WAIT).await.unwrap();
    assert_eq!(data, b"reply");
    assert_eq!(from, to_b);
}
