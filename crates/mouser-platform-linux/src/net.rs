//! Interface enumeration and physical-LAN classification (research R3).

use mouser_platform::{InterfaceKind, NetInterface, WatchHandle};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, Sender};

/// Name prefixes that are never a physical LAN, whatever sysfs says.
const VIRTUAL_PREFIXES: &[&str] = &[
    "tun",
    "tap",
    "wg",
    "tailscale",
    "zt",
    "docker",
    "br-",
    "virbr",
    "veth",
    "vmnet",
    "vboxnet",
];

/// `ARPHRD_ETHER`, the `type` of Ethernet and Wi-Fi hardware.
const ARPHRD_ETHER: &str = "1";

pub(crate) fn classify(sys_class_net: &Path, name: &str) -> InterfaceKind {
    let dir = sys_class_net.join(name);
    if name == "lo" {
        InterfaceKind::Loopback
    } else if VIRTUAL_PREFIXES.iter().any(|p| name.starts_with(p)) {
        InterfaceKind::Virtual
    } else if dir.join("device").exists()
        && (dir.join("wireless").is_dir()
            || std::fs::read_to_string(dir.join("type")).is_ok_and(|t| t.trim() == ARPHRD_ETHER))
    {
        InterfaceKind::PhysicalLan
    } else {
        InterfaceKind::Virtual
    }
}

/// One entry per interface name, collecting its addresses.
pub(crate) fn group(
    sys_class_net: &Path,
    addrs: impl IntoIterator<Item = (String, IpAddr)>,
) -> Vec<NetInterface> {
    let mut out: Vec<NetInterface> = Vec::new();
    for (name, ip) in addrs {
        match out.iter_mut().find(|n| n.name == name) {
            Some(existing) => existing.addrs.push(ip),
            None => out.push(NetInterface {
                kind: classify(sys_class_net, &name),
                name,
                addrs: vec![ip],
            }),
        }
    }
    out
}

pub(crate) fn interfaces(sys_class_net: &Path) -> Vec<NetInterface> {
    group(
        sys_class_net,
        if_addrs::get_if_addrs()
            .expect("getifaddrs")
            .into_iter()
            .map(|i| (i.name.clone(), i.ip())),
    )
}

/// How often the interface list is compared for changes.
const POLL: std::time::Duration = std::time::Duration::from_secs(2);

/// Fires `tx` whenever the interfaces or their addresses change, until the
/// returned handle is dropped.
pub(crate) fn watch(sys_class_net: PathBuf, tx: Sender<()>) -> WatchHandle {
    watch_with(move || interfaces(&sys_class_net), POLL, tx)
}

pub(crate) fn watch_with(
    mut snapshot: impl FnMut() -> Vec<NetInterface> + Send + 'static,
    every: std::time::Duration,
    tx: Sender<()>,
) -> WatchHandle {
    let fingerprint = |all: Vec<NetInterface>| {
        let mut seen: Vec<_> = all.into_iter().map(|i| (i.name, i.addrs)).collect();
        seen.sort();
        seen
    };
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let thread = std::thread::spawn(move || {
        let mut last = fingerprint(snapshot());
        while stopped.recv_timeout(every) == Err(RecvTimeoutError::Timeout) {
            let now = fingerprint(snapshot());
            if now != last {
                last = now;
                let _ = tx.send(());
            }
        }
    });
    WatchHandle::new(move || {
        drop(stop);
        thread.join().unwrap();
    })
}
