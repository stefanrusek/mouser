//! Throwaway probe for Mouser plan phase. NOT production code.
//!
//! Modes:
//!   ifaces                         enumerate interfaces (if-addrs) + classify
//!   register <desk> <secs> <mode>  register _mouser-spike._udp; mode = default | lan
//!   browse <secs>                  browse _mouser-spike._udp and print resolved records
//!   same <desk> <mode>             register + browse in one process
use anyhow::Result;
use if_addrs::Interface;
use mdns_sd::{IfKind, IfPredicate, ServiceDaemon, ServiceEvent, ServiceInfo};
use std::time::{Duration, Instant};

const TY: &str = "_mouser-spike._udp.local.";

#[derive(Debug, PartialEq)]
enum Class {
    Loopback,
    PhysicalLanCandidate,
    Virtual(&'static str),
}

/// Name-prefix heuristic. On macOS a robust implementation would cross-check with
/// SystemConfiguration (SCNetworkInterfaceGetInterfaceType = Ethernet/IEEE80211);
/// on Windows, use the adapter type / "vEthernet"/"Hyper-V"/"VMware"/"VirtualBox" in the friendly name.
fn classify(i: &Interface) -> Class {
    // NB: if-addrs is_loopback() is false for fe80::1%lo0, so also match the name.
    if i.is_loopback() || i.name.starts_with("lo") {
        return Class::Loopback;
    }
    const VIRTUAL: &[(&str, &str)] = &[
        ("utun", "VPN/tunnel (utun)"),
        ("ipsec", "VPN (ipsec)"),
        ("ppp", "VPN (ppp)"),
        ("tun", "VPN (tun)"),
        ("tap", "VPN/VM (tap)"),
        ("wg", "WireGuard"),
        ("tailscale", "Tailscale"),
        ("zt", "ZeroTier"),
        ("gif", "tunnel (gif)"),
        ("stf", "6to4 (stf)"),
        ("bridge", "bridge (VM/Thunderbolt/Internet Sharing)"),
        ("awdl", "Apple Wireless Direct Link"),
        ("llw", "Apple low-latency WLAN"),
        ("anpi", "Apple NPI (internal)"),
        ("ap", "Apple Wi-Fi AP / hotspot"),
        ("vmnet", "VMware"),
        ("vboxnet", "VirtualBox"),
        ("docker", "Docker"),
        ("br-", "Docker/Linux bridge"),
        ("virbr", "libvirt bridge"),
        ("veth", "container veth"),
        ("vEthernet", "Hyper-V/WSL vEthernet"),
    ];
    for (p, why) in VIRTUAL {
        if i.name.starts_with(p) {
            return Class::Virtual(why);
        }
    }
    if i.is_p2p() {
        return Class::Virtual("point-to-point flag");
    }
    Class::PhysicalLanCandidate
}

fn ifaces() -> Result<()> {
    let mut v = if_addrs::get_if_addrs()?;
    v.sort_by(|a, b| a.name.cmp(&b.name));
    println!("{:<9} {:<34} {:>5} {:<5} {:<4} {:<5} class", "name", "ip", "idx", "up", "p2p", "llocal");
    for i in &v {
        println!(
            "{:<9} {:<34} {:>5} {:<5} {:<4} {:<5} {:?}",
            i.name,
            i.ip().to_string(),
            i.index.map(|x| x.to_string()).unwrap_or_default(),
            i.is_oper_up(),
            i.is_p2p(),
            i.is_link_local(),
            classify(i)
        );
    }
    Ok(())
}

fn lan_predicate() -> IfPredicate {
    IfPredicate::new(|i: &Interface| classify(i) == Class::PhysicalLanCandidate)
}

fn daemon(mode: &str) -> Result<ServiceDaemon> {
    let d = ServiceDaemon::new()?;
    if mode == "lan" {
        d.disable_interface(IfKind::All)?;
        d.enable_interface(IfKind::Predicate(lan_predicate()))?;
    }
    Ok(d)
}

fn register(d: &ServiceDaemon, desk: &str) -> Result<String> {
    let host = format!("mouser-spike-{}.local.", std::process::id());
    let props = [("desk", desk), ("ver", "0.0.1"), ("id", "abcd1234")];
    let info = ServiceInfo::new(TY, &format!("{desk}-{}", std::process::id()), &host, "", 47999, &props[..])?
        .enable_addr_auto();
    let full = info.get_fullname().to_string();
    d.register(info)?;
    println!("registered {full} host={host} port=47999 txt={props:?}");
    Ok(full)
}

fn browse(d: &ServiceDaemon, secs: u64) -> Result<()> {
    let rx = d.browse(TY)?;
    let end = Instant::now() + Duration::from_secs(secs);
    while let Ok(ev) = rx.recv_deadline(end) {
        match ev {
            ServiceEvent::ServiceResolved(r) => {
                let mut addrs: Vec<String> = r.addresses.iter().map(|a| format!("{a:?}")).collect();
                addrs.sort();
                println!(
                    "RESOLVED {} host={} port={} desk={:?}\n   addresses ({}):",
                    r.fullname,
                    r.host,
                    r.port,
                    r.txt_properties.get_property_val_str("desk"),
                    addrs.len()
                );
                for a in addrs {
                    println!("     {a}");
                }
            }
            ServiceEvent::ServiceFound(_, f) => println!("found {f}"),
            ServiceEvent::ServiceRemoved(_, f) => println!("removed {f}"),
            other => println!("{other:?}"),
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("ifaces") => ifaces(),
        Some("register") => {
            let d = daemon(&a[4])?;
            let full = register(&d, &a[2])?;
            std::thread::sleep(Duration::from_secs(a[3].parse()?));
            // Goodbye packets: explicit unregister and wait, then shutdown and wait.
            let st = d.unregister(&full)?.recv_timeout(Duration::from_secs(3));
            println!("unregister status: {st:?}");
            let st = d.shutdown()?.recv_timeout(Duration::from_secs(3));
            println!("shutdown status: {st:?}");
            Ok(())
        }
        Some("browse") => browse(&daemon("default")?, a[2].parse()?),
        Some("same") => {
            let d = daemon(&a[3])?;
            register(&d, &a[2])?;
            browse(&d, 4)
        }
        _ => anyhow::bail!("usage"),
    }
}
