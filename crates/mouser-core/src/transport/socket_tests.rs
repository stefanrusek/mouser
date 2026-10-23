//! The endpoint's IPv4 and IPv6 sockets (see `socket`).

use std::cell::Cell;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use super::socket::Sockets;
use super::tests::hold_wildcard_port;

#[test]
fn a_bind_failure_names_the_address_and_the_os_error() {
    let held = hold_wildcard_port();
    let port = held.0.local_addr().unwrap().port();
    let error = Sockets::bind(Ipv4Addr::UNSPECIFIED, Some(Ipv6Addr::UNSPECIFIED), port)
        .err()
        .unwrap();
    assert!(error.starts_with(&format!("0.0.0.0:{port}: ")), "{error}");
}

#[tokio::test]
async fn without_ipv6_the_endpoint_is_ipv4_only() {
    let unassigned: Ipv6Addr = "2001:db8::1".parse().unwrap();
    let sockets = Sockets::bind(Ipv4Addr::LOCALHOST, Some(unassigned), 0).unwrap();
    let addr = sockets.quinn().local_addr().unwrap();
    assert_eq!(addr.ip(), Ipv4Addr::LOCALHOST);
}

#[tokio::test]
async fn an_ephemeral_port_taken_on_ipv6_is_given_up_for_another() {
    let ipv6_attempts = Cell::new(0);
    let bind = |addr: SocketAddr| {
        if addr.is_ipv6() {
            ipv6_attempts.set(ipv6_attempts.get() + 1);
            if ipv6_attempts.get() == 1 {
                return Err(io::ErrorKind::AddrInUse.into());
            }
        }
        super::udp_socket(addr)
    };
    let sockets =
        Sockets::bind_with(Ipv4Addr::UNSPECIFIED, Some(Ipv6Addr::UNSPECIFIED), 0, bind).unwrap();
    assert_eq!(ipv6_attempts.get(), 2);
    assert!(sockets.quinn().local_addr().unwrap().is_ipv6());
}

#[tokio::test]
async fn the_pair_is_writable_when_both_sockets_are() {
    let sockets = Sockets::bind(Ipv4Addr::LOCALHOST, Some(Ipv6Addr::LOCALHOST), 0).unwrap();
    let mut poller = sockets.quinn().create_io_poller();
    let writable = std::future::poll_fn(|cx| poller.as_mut().poll_writable(cx));
    tokio::time::timeout(std::time::Duration::from_secs(5), writable)
        .await
        .expect("both sockets become writable")
        .unwrap();
}
