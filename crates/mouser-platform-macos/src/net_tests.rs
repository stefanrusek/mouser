use crate::net::{classify, interfaces};
use mouser_platform::InterfaceKind::{Loopback, PhysicalLan, Virtual};

#[test]
fn ethernet_and_wifi_hardware_is_physical_lan() {
    assert_eq!(classify("en0", Some("Ethernet")), PhysicalLan);
    assert_eq!(classify("en1", Some("IEEE80211")), PhysicalLan);
}

#[test]
fn virtual_names_are_virtual_even_with_lan_hardware_type() {
    for name in [
        "utun3", "bridge0", "awdl0", "llw0", "anpi1", "ap1", "vmnet8", "docker0",
    ] {
        assert_eq!(classify(name, Some("Ethernet")), Virtual, "{name}");
    }
}

#[test]
fn non_lan_hardware_or_unknown_is_virtual() {
    assert_eq!(classify("en5", Some("Bridge")), Virtual);
    assert_eq!(classify("en6", None), Virtual);
}

#[test]
fn loopback_is_loopback_including_its_link_local_address() {
    assert_eq!(classify("lo0", None), Loopback);
    let lo = interfaces().into_iter().find(|i| i.name == "lo0").unwrap();
    assert_eq!(lo.kind, Loopback);
    assert!(lo.addrs.contains(&"127.0.0.1".parse().unwrap()));
}

#[test]
fn enumeration_groups_addresses_per_interface() {
    let all = interfaces();
    let mut names: Vec<_> = all.iter().map(|i| i.name.clone()).collect();
    let n = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), n, "one entry per interface");
    assert!(all.iter().all(|i| !i.addrs.is_empty()));
}
