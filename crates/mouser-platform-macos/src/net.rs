//! Interface enumeration and physical-LAN classification (research R3).

use mouser_platform::{InterfaceKind, NetInterface};
use std::collections::HashMap;
use system_configuration::network_configuration::get_interfaces;

/// Name prefixes that are never a physical LAN, whatever the hardware type.
const VIRTUAL_PREFIXES: &[&str] = &[
    "utun",
    "ipsec",
    "ppp",
    "tun",
    "tap",
    "wg",
    "tailscale",
    "zt",
    "gif",
    "stf",
    "bridge",
    "awdl",
    "llw",
    "anpi",
    "ap",
    "vmnet",
    "vboxnet",
    "docker",
    "br-",
    "virbr",
    "veth",
];

/// `SCNetworkInterfaceGetInterfaceType` values of real LAN hardware.
const LAN_TYPES: &[&str] = &["Ethernet", "IEEE80211"];

pub(crate) fn classify(name: &str, sc_type: Option<&str>) -> InterfaceKind {
    if name.starts_with("lo") {
        InterfaceKind::Loopback
    } else if VIRTUAL_PREFIXES.iter().any(|p| name.starts_with(p)) {
        InterfaceKind::Virtual
    } else if sc_type.is_some_and(|t| LAN_TYPES.contains(&t)) {
        InterfaceKind::PhysicalLan
    } else {
        InterfaceKind::Virtual
    }
}

/// BSD name -> SystemConfiguration interface type string.
fn hardware_types() -> HashMap<String, String> {
    get_interfaces()
        .iter()
        .filter_map(|i| {
            Some((
                i.bsd_name()?.to_string(),
                i.interface_type_string()?.to_string(),
            ))
        })
        .collect()
}

pub(crate) fn interfaces() -> Vec<NetInterface> {
    let types = hardware_types();
    let mut out: Vec<NetInterface> = Vec::new();
    for iface in if_addrs::get_if_addrs().expect("getifaddrs") {
        let ip = iface.ip();
        match out.iter_mut().find(|n| n.name == iface.name) {
            Some(existing) => existing.addrs.push(ip),
            None => out.push(NetInterface {
                kind: classify(&iface.name, types.get(&iface.name).map(String::as_str)),
                name: iface.name,
                addrs: vec![ip],
            }),
        }
    }
    out
}
