use crate::netwatch::{open_netlink, relevant, watch, watch_netlink};
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixDatagram;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

const RTM_NEWLINK: u16 = 16;
const RTM_DELLINK: u16 = 17;
const RTM_NEWADDR: u16 = 20;
const RTM_DELADDR: u16 = 21;
const RTM_NEWROUTE: u16 = 24;

/// One netlink message of `kind` with `payload` bytes after the 16-byte header.
fn message(kind: u16, payload: usize) -> Vec<u8> {
    let mut out = ((16 + payload) as u32).to_ne_bytes().to_vec();
    out.extend(kind.to_ne_bytes());
    out.extend([0u8; 10]);
    out.extend(vec![0u8; payload]);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out
}

#[test]
fn link_and_address_messages_are_relevant() {
    for kind in [RTM_NEWLINK, RTM_DELLINK, RTM_NEWADDR, RTM_DELADDR] {
        assert!(relevant(&message(kind, 0)), "{kind}");
    }
}

#[test]
fn routes_and_other_messages_are_not_relevant() {
    assert!(!relevant(&message(RTM_NEWROUTE, 8)));
    assert!(!relevant(&[]));
}

#[test]
fn a_relevant_message_after_others_in_one_datagram_is_found() {
    let mut datagram = message(RTM_NEWROUTE, 5);
    datagram.extend(message(RTM_NEWADDR, 0));
    assert!(relevant(&datagram));
}

#[test]
fn a_message_that_claims_less_than_its_header_ends_the_scan() {
    let mut broken = message(RTM_NEWROUTE, 0);
    broken[..4].copy_from_slice(&0u32.to_ne_bytes());
    broken.extend(message(RTM_NEWADDR, 0));
    assert!(!relevant(&broken));
}

fn fake_netlink() -> (UnixDatagram, OwnedFd) {
    let (ours, theirs) = UnixDatagram::pair().unwrap();
    (ours, OwnedFd::from(theirs))
}

#[test]
fn relevant_netlink_datagrams_fire_the_watch_once_each() {
    let (kernel, socket) = fake_netlink();
    let (tx, rx) = channel();
    let handle = watch_netlink(socket, tx);
    kernel.send(&message(RTM_NEWADDR, 4)).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    kernel.send(&message(RTM_DELLINK, 0)).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(handle);
    assert_eq!(
        rx.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Disconnected)
    );
}

#[test]
fn irrelevant_netlink_datagrams_do_not_fire_the_watch() {
    let (kernel, socket) = fake_netlink();
    let (tx, rx) = channel();
    let handle = watch_netlink(socket, tx);
    kernel.send(&message(RTM_NEWROUTE, 0)).unwrap();
    kernel.send(&message(RTM_NEWLINK, 0)).unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        rx.recv_timeout(Duration::from_millis(100)),
        Err(RecvTimeoutError::Timeout)
    );
    drop(handle);
}

#[test]
fn the_kernel_socket_opens_and_stays_quiet_without_changes() {
    let socket = open_netlink();
    let (tx, rx) = channel();
    let handle = watch_netlink(socket, tx);
    drop(handle);
    assert!(rx.try_recv().is_err());
}

#[test]
fn without_a_system_bus_the_watch_still_follows_the_network() {
    let env = HashMap::from([(
        "DBUS_SYSTEM_BUS_ADDRESS".to_string(),
        "unix:path=/nonexistent/bus".to_string(),
    )]);
    let (tx, _rx) = channel();
    drop(watch(&env, tx));
}
