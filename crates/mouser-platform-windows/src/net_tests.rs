use crate::net::{classify, interfaces};
use mouser_platform::InterfaceKind::{Loopback, PhysicalLan, Virtual};
use windows::Win32::NetworkManagement::IpHelper::{
    IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE1394, IF_TYPE_IEEE80211, IF_TYPE_PPP,
    IF_TYPE_PROP_VIRTUAL, IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL,
};

#[test]
fn ethernet_and_wifi_iftypes_are_physical_lan() {
    assert_eq!(
        classify(
            IF_TYPE_ETHERNET_CSMACD,
            "Ethernet",
            "Intel(R) Ethernet Connection I219-V"
        ),
        PhysicalLan
    );
    assert_eq!(
        classify(IF_TYPE_IEEE80211, "Wi-Fi", "Intel(R) Wi-Fi 6E AX211 160MHz"),
        PhysicalLan
    );
}

#[test]
fn hyperv_vm_and_vpn_adapters_are_virtual_even_as_ethernet() {
    for (name, description) in [
        (
            "vEthernet (Default Switch)",
            "Hyper-V Virtual Ethernet Adapter",
        ),
        ("Ethernet 3", "Hyper-V Virtual Ethernet Adapter #2"),
        (
            "VMware Network Adapter VMnet8",
            "VMware Virtual Ethernet Adapter for VMnet8",
        ),
        ("Ethernet 4", "VirtualBox Host-Only Ethernet Adapter"),
        ("Ethernet 5", "TAP-Windows Adapter V9"),
        ("wg0", "WireGuard Tunnel"),
        ("Tailscale", "Tailscale Tunnel"),
    ] {
        assert_eq!(
            classify(IF_TYPE_ETHERNET_CSMACD, name, description),
            Virtual,
            "{name}"
        );
    }
}

#[test]
fn tunnel_ppp_and_software_iftypes_are_virtual() {
    for if_type in [IF_TYPE_TUNNEL, IF_TYPE_PPP, IF_TYPE_PROP_VIRTUAL] {
        assert_eq!(
            classify(
                if_type,
                "Local Area Connection* 1",
                "Microsoft Teredo Tunneling Adapter"
            ),
            Virtual
        );
    }
}

#[test]
fn other_iftypes_are_not_physical_lan() {
    assert_eq!(
        classify(IF_TYPE_IEEE1394, "Ethernet 9", "1394 Net Adapter"),
        Virtual
    );
}

#[test]
fn loopback_is_loopback_including_its_enumerated_address() {
    assert_eq!(
        classify(
            IF_TYPE_SOFTWARE_LOOPBACK,
            "Loopback Pseudo-Interface 1",
            "Software Loopback Interface 1"
        ),
        Loopback
    );
    let lo = interfaces()
        .into_iter()
        .find(|i| i.kind == Loopback)
        .unwrap();
    assert!(lo.addrs.contains(&"127.0.0.1".parse().unwrap()), "{lo:?}");
    assert!(lo.addrs.contains(&"::1".parse().unwrap()), "{lo:?}");
}

#[test]
fn enumeration_has_one_entry_per_adapter() {
    let all = interfaces();
    let mut names: Vec<_> = all.iter().map(|i| i.name.clone()).collect();
    let n = names.len();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), n, "one entry per adapter");
}
