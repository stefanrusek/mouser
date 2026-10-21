use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use mouser_platform::{InterfaceKind, NetInterface};

use super::filter::{advertised_addrs, is_enabled};

fn v4(d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(192, 168, 1, d))
}

fn fe80(n: u16) -> IpAddr {
    IpAddr::V6(Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, n))
}

fn iface(name: &str, kind: InterfaceKind, addrs: Vec<IpAddr>) -> NetInterface {
    NetInterface {
        name: name.into(),
        kind,
        addrs,
    }
}

fn interfaces() -> Vec<NetInterface> {
    vec![
        iface("eth0", InterfaceKind::PhysicalLan, vec![v4(10), fe80(1)]),
        iface("eth1", InterfaceKind::PhysicalLan, vec![v4(11), fe80(2)]),
        iface("utun3", InterfaceKind::Virtual, vec![v4(99), fe80(3)]),
        iface(
            "lo0",
            InterfaceKind::Loopback,
            vec![Ipv4Addr::LOCALHOST.into()],
        ),
    ]
}

#[test]
fn only_physical_lan_interfaces_are_enabled() {
    let list = interfaces();
    assert!(is_enabled(&list, "eth0", v4(10)));
    assert!(is_enabled(&list, "eth1", v4(11)));
    assert!(!is_enabled(&list, "utun3", v4(99)));
    assert!(!is_enabled(&list, "lo0", Ipv4Addr::LOCALHOST.into()));
    assert!(!is_enabled(&list, "missing", v4(10)));
}

#[test]
fn a_link_local_address_belongs_to_its_own_interface_only() {
    let list = interfaces();
    assert!(is_enabled(&list, "eth0", fe80(1)));
    assert!(!is_enabled(&list, "eth0", fe80(2)));
    assert!(!is_enabled(&list, "eth1", fe80(1)));
}

#[test]
fn ipv4_is_preferred_for_advertising() {
    assert_eq!(advertised_addrs(&interfaces()), vec![v4(10), v4(11)]);
}

#[test]
fn link_local_is_advertised_only_when_there_is_no_ipv4() {
    let list = vec![iface("eth0", InterfaceKind::PhysicalLan, vec![fe80(1)])];
    assert_eq!(advertised_addrs(&list), vec![fe80(1)]);
    assert!(advertised_addrs(&[]).is_empty());
}
