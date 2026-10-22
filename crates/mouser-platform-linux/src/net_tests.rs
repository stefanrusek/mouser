use crate::net::{classify, group, interfaces};
use mouser_platform::InterfaceKind::{Loopback, PhysicalLan, Virtual};
use std::net::IpAddr;
use std::path::Path;

/// A fake `/sys/class/net` with one entry per `(name, has_device, type, wireless)`.
fn sysfs(entries: &[(&str, bool, Option<&str>, bool)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (name, device, kind, wireless) in entries {
        let dir = root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        if *device {
            std::fs::create_dir_all(dir.join("device")).unwrap();
        }
        if let Some(kind) = kind {
            std::fs::write(dir.join("type"), format!("{kind}\n")).unwrap();
        }
        if *wireless {
            std::fs::create_dir_all(dir.join("wireless")).unwrap();
        }
    }
    root
}

#[test]
fn ethernet_hardware_is_physical_lan() {
    let root = sysfs(&[("enp3s0", true, Some("1"), false)]);
    assert_eq!(classify(root.path(), "enp3s0"), PhysicalLan);
}

#[test]
fn wireless_hardware_is_physical_lan() {
    let root = sysfs(&[("wlp2s0", true, None, true)]);
    assert_eq!(classify(root.path(), "wlp2s0"), PhysicalLan);
}

#[test]
fn loopback_is_loopback() {
    let root = sysfs(&[("lo", false, Some("772"), false)]);
    assert_eq!(classify(root.path(), "lo"), Loopback);
}

#[test]
fn virtual_names_are_virtual_even_when_they_look_like_ethernet_hardware() {
    let names = [
        "tun0",
        "tap1",
        "wg0",
        "tailscale0",
        "zt5u4",
        "docker0",
        "br-1a2b3c",
        "virbr0",
        "veth12ab",
        "vmnet8",
        "vboxnet0",
    ];
    let entries: Vec<_> = names.iter().map(|n| (*n, true, Some("1"), false)).collect();
    let root = sysfs(&entries);
    for name in names {
        assert_eq!(classify(root.path(), name), Virtual, "{name}");
    }
}

#[test]
fn an_interface_without_a_device_or_of_another_type_is_virtual() {
    let root = sysfs(&[
        ("eno1", false, Some("1"), false),
        ("ib0", true, Some("32"), false),
        ("sit0", true, None, false),
    ]);
    for name in ["eno1", "ib0", "sit0", "missing0"] {
        assert_eq!(classify(root.path(), name), Virtual, "{name}");
    }
}

#[test]
fn addresses_are_grouped_per_interface_and_classified() {
    let root = sysfs(&[("eth0", true, Some("1"), false)]);
    let ip = |s: &str| s.parse::<IpAddr>().unwrap();
    let all = group(
        root.path(),
        [
            ("eth0".to_string(), ip("192.168.1.5")),
            ("lo".to_string(), ip("127.0.0.1")),
            ("eth0".to_string(), ip("fe80::1")),
        ],
    );
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].name, "eth0");
    assert_eq!(all[0].kind, PhysicalLan);
    assert_eq!(all[0].addrs, [ip("192.168.1.5"), ip("fe80::1")]);
    assert_eq!(all[1].kind, Loopback);
}

#[test]
fn the_machine_has_a_loopback_interface() {
    let all = interfaces(Path::new("/sys/class/net"));
    assert!(all.iter().all(|i| !i.addrs.is_empty()));
    let lo = all.iter().find(|i| i.name == "lo").unwrap();
    assert_eq!(lo.kind, Loopback);
    assert!(lo.addrs.contains(&"127.0.0.1".parse().unwrap()));
}
