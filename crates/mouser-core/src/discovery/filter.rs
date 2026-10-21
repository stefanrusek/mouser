//! The physical-LAN interface predicate (research R3).

use std::net::IpAddr;

use mouser_platform::{InterfaceKind, NetInterface};

/// Whether the address `ip` configured on interface `name` may be used for
/// discovery: the interface must be physical LAN and the address must be one
/// of that interface's own (so a link-local address never leaks to another).
pub fn is_enabled(interfaces: &[NetInterface], name: &str, ip: IpAddr) -> bool {
    interfaces
        .iter()
        .any(|i| i.name == name && i.kind == InterfaceKind::PhysicalLan && i.addrs.contains(&ip))
}

/// Addresses to advertise: those of physical LAN interfaces, IPv4 when there
/// is any, otherwise the IPv6 (link-local) ones.
pub fn advertised_addrs(interfaces: &[NetInterface]) -> Vec<IpAddr> {
    let all: Vec<IpAddr> = interfaces
        .iter()
        .filter(|i| i.kind == InterfaceKind::PhysicalLan)
        .flat_map(|i| i.addrs.iter().copied())
        .collect();
    let v4: Vec<IpAddr> = all.iter().copied().filter(IpAddr::is_ipv4).collect();
    if v4.is_empty() { all } else { v4 }
}
