use std::time::{Duration, Instant};

use mouser_platform::{InterfaceKind, NetInterface};

use super::*;

fn advert(n: u8, desk: Option<(u8, &str)>, port: u16) -> Advert {
    Advert {
        computer: ComputerId([n; 16]),
        display_name: format!("Computer {n}"),
        protocol: 1,
        desk: desk.map(|(d, name)| (DeskId([d; 16]), name.to_string())),
        port,
    }
}

fn addr(n: u8) -> IpAddr {
    IpAddr::from([192, 168, 7, n])
}

#[test]
fn the_service_carries_the_contract_txt_keys() {
    let ad = advert(1, Some((9, "Home office")), 47801);
    let info = service_info(&ad, &[addr(1)]);
    assert_eq!(info.get_type(), SERVICE_TYPE);
    assert_eq!(info.get_fullname(), format!("Computer 1.{SERVICE_TYPE}"));
    assert_eq!(info.get_port(), 47801);
    let prop = |k: &str| info.get_property_val_str(k).map(str::to_string);
    assert_eq!(prop("v").as_deref(), Some("1"));
    assert_eq!(prop("cid"), Some(ComputerId([1; 16]).to_string()));
    assert_eq!(prop("desk"), Some(DeskId([9; 16]).to_string()));
    assert_eq!(prop("dn").as_deref(), Some("Home office"));
    assert_eq!(prop("port").as_deref(), Some("47801"));
}

#[test]
fn an_unjoined_computer_has_an_empty_desk() {
    let info = service_info(&advert(2, None, 5000), &[addr(2)]);
    assert_eq!(info.get_property_val_str("desk"), Some(""));
    let found = found_from(&info.as_resolved_service()).unwrap();
    assert_eq!(found.desk, None);
}

#[test]
fn a_resolved_record_becomes_a_found_computer_with_ipv4_first() {
    let ad = advert(3, Some((9, "Home office")), 5001);
    let v6: IpAddr = "fe80::1".parse().unwrap();
    let info = service_info(&ad, &[v6, addr(3)]);
    let found = found_from(&info.as_resolved_service()).unwrap();
    assert_eq!(found.computer, ComputerId([3; 16]));
    assert_eq!(found.display_name, "Computer 3");
    assert_eq!(found.protocol, 1);
    assert_eq!(
        found.desk,
        Some((DeskId([9; 16]), "Home office".to_string()))
    );
    assert_eq!(found.addrs.len(), 2);
    assert_eq!(found.addrs[0], SocketAddr::new(addr(3), 5001));
    assert!(found.addrs[1].is_ipv6());
    assert_eq!(found.addrs[1].port(), 5001);
}

#[test]
fn records_that_are_not_mouser_records_are_ignored() {
    let keys: Vec<(&str, String)> = txt(&advert(4, Some((9, "D")), 5002));
    for broken in 0..keys.len() {
        let mut props = keys.clone();
        props.remove(broken);
        let host = "mouser-x.local.";
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            "Computer 4",
            host,
            addr(4),
            5002,
            props.as_slice(),
        )
        .unwrap();
        assert_eq!(
            found_from(&info.as_resolved_service()),
            None,
            "missing {}",
            keys[broken].0
        );
    }
    for (key, bad) in [("v", "x"), ("cid", "zz"), ("desk", "zz"), ("port", "99999")] {
        let props: Vec<(&str, String)> = keys
            .iter()
            .map(|(k, v)| {
                (
                    *k,
                    if *k == key {
                        bad.to_string()
                    } else {
                        v.clone()
                    },
                )
            })
            .collect();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            "Computer 4",
            "mouser-x.local.",
            addr(4),
            5002,
            props.as_slice(),
        )
        .unwrap();
        assert_eq!(found_from(&info.as_resolved_service()), None, "bad {key}");
    }
    let other_type = ServiceInfo::new(
        "_other._udp.local.",
        "Computer 4",
        "mouser-x.local.",
        addr(4),
        5002,
        keys.as_slice(),
    )
    .unwrap();
    assert_eq!(found_from(&other_type.as_resolved_service()), None);
}

fn found(n: u8, desk: Option<(u8, &str)>) -> Found {
    Found {
        computer: ComputerId([n; 16]),
        display_name: format!("Computer {n}"),
        protocol: 1,
        desk: desk.map(|(d, name)| (DeskId([d; 16]), name.to_string())),
        addrs: Vec::new(),
    }
}

#[test]
fn browse_results_group_computers_by_desk() {
    let all = [
        found(1, Some((1, "Home"))),
        found(2, Some((1, "Home"))),
        found(3, Some((2, "Home"))),
        found(4, None),
    ];
    let desks = group_by_desk(&all, None);
    assert_eq!(desks.len(), 2, "same names stay separate desks");
    assert_eq!(desks[0].desk_id, DeskId([1; 16]));
    assert_eq!(desks[0].members, ["Computer 1", "Computer 2"]);
    assert_eq!(desks[1].members, ["Computer 3"]);
    let others = group_by_desk(&all, Some(DeskId([1; 16])));
    assert_eq!(others.len(), 1);
    assert_eq!(others[0].desk_id, DeskId([2; 16]));
}

fn host_interfaces() -> Vec<NetInterface> {
    let mut by_name: BTreeMap<String, NetInterface> = BTreeMap::new();
    for i in if_addrs::get_if_addrs().unwrap() {
        by_name
            .entry(i.name.clone())
            .or_insert_with(|| NetInterface {
                name: i.name.clone(),
                kind: if i.is_loopback() {
                    InterfaceKind::Loopback
                } else {
                    InterfaceKind::PhysicalLan
                },
                addrs: Vec::new(),
            })
            .addrs
            .push(i.ip());
    }
    by_name.into_values().collect()
}

fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    assert!(
        wait_for(Duration::from_secs(30), &mut done),
        "timed out waiting for {what}"
    );
}

fn wait_for(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while !done() {
        if Instant::now() >= end {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    true
}

#[test]
fn real_mdns_announces_browses_and_unregisters_on_stop() {
    let (a_id, b_id) = (ComputerId([0xa1; 16]), ComputerId([0xb2; 16]));
    let b = MdnsDiscovery::new(b_id, host_interfaces());
    // Multicast is lossy: if the goodbye is lost, announce and stop again.
    let mut gone = false;
    for _ in 0..3 {
        let a = MdnsDiscovery::new(a_id, host_interfaces());
        a.announce(&advert(0xa1, Some((9, "Mdns test desk")), 47811));
        wait_until("B to see A", || {
            b.found().iter().any(|f| f.computer == a_id)
        });
        let seen = b.found().into_iter().find(|f| f.computer == a_id).unwrap();
        assert_eq!(
            seen.desk,
            Some((DeskId([9; 16]), "Mdns test desk".to_string()))
        );
        assert!(seen.addrs.iter().all(|s| s.port() == 47811));
        assert!(
            a.found().iter().all(|f| f.computer != a_id),
            "own record is hidden"
        );
        // A second announce replaces the first.
        a.announce(&advert(0xa1, None, 47811));
        wait_until("B to see A unjoined", || {
            b.found()
                .iter()
                .any(|f| f.computer == a_id && f.desk.is_none())
        });
        a.stop();
        gone = wait_for(Duration::from_secs(10), || {
            b.found().iter().all(|f| f.computer != a_id)
        });
        if gone {
            break;
        }
    }
    assert!(gone, "A never disappeared from B's list after stopping");
    b.stop();
}

#[test]
fn announcements_follow_the_interfaces_the_computer_has_now() {
    let physical = |name: &str, addr: [u8; 4]| NetInterface {
        name: name.into(),
        kind: InterfaceKind::PhysicalLan,
        addrs: vec![std::net::IpAddr::from(addr)],
    };
    let vpn = NetInterface {
        name: "utun3".into(),
        kind: InterfaceKind::Virtual,
        addrs: vec![std::net::IpAddr::from([10, 8, 0, 2])],
    };
    let discovery = MdnsDiscovery::new(
        ComputerId([0xc3; 16]),
        vec![physical("en0", [192, 168, 1, 5])],
    );
    assert_eq!(
        discovery.announced_addrs(),
        [std::net::IpAddr::from([192, 168, 1, 5])]
    );
    // A VPN starts and the computer moves from Wi-Fi to Ethernet.
    discovery.interfaces_changed(vec![vpn, physical("en7", [192, 168, 7, 9])]);
    assert_eq!(
        discovery.announced_addrs(),
        [std::net::IpAddr::from([192, 168, 7, 9])]
    );
    discovery.stop();
}
