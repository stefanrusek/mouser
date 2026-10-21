//! LAN discovery with mDNS/DNS-SD (contracts/wire-protocol.md "Discovery").
//!
//! The engine talks to the [`Discovery`] trait; [`MdnsDiscovery`] is the real
//! implementation. Tests with a simulated network back the trait with an
//! in-process registry, because multicast is not routed through `SimNet`.

mod filter;
#[cfg(test)]
mod filter_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr, SocketAddrV6};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mdns_sd::{
    IfKind, IfPredicate, ResolvedService, ScopedIp, ServiceDaemon, ServiceEvent, ServiceInfo,
};
use mouser_platform::NetInterface;
use mouser_proto::control::DiscoveredDesk;
use mouser_proto::ids::{ComputerId, DeskId};

pub use filter::{advertised_addrs, is_enabled};

/// DNS-SD service type of every Mouser computer.
pub const SERVICE_TYPE: &str = "_mouser._udp.local.";

/// How long to wait for the confirmation of an unregister or shutdown.
const CONFIRM: Duration = Duration::from_secs(3);

/// What this computer announces about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advert {
    pub computer: ComputerId,
    pub display_name: String,
    pub protocol: u16,
    /// Desk id and name; `None` while unjoined.
    pub desk: Option<(DeskId, String)>,
    pub port: u16,
}

/// Another computer seen on the network.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub computer: ComputerId,
    pub display_name: String,
    pub protocol: u16,
    pub desk: Option<(DeskId, String)>,
    /// Dialable addresses, IPv4 first.
    pub addrs: Vec<SocketAddr>,
}

/// Announce this computer and list the others.
pub trait Discovery: Send + Sync + 'static {
    /// Start announcing, replacing any earlier announcement.
    fn announce(&self, advert: &Advert);
    /// The other computers currently visible.
    fn found(&self) -> Vec<Found>;
    /// Stop announcing and wait until the network has been told.
    fn stop(&self);
}

/// TXT records: `v`, `cid`, `desk`, `dn`, `port`.
fn txt(advert: &Advert) -> Vec<(&'static str, String)> {
    let (desk, desk_name) = match &advert.desk {
        Some((id, name)) => (id.to_string(), name.clone()),
        None => (String::new(), String::new()),
    };
    vec![
        ("v", advert.protocol.to_string()),
        ("cid", advert.computer.to_string()),
        ("desk", desk),
        ("dn", desk_name),
        ("port", advert.port.to_string()),
    ]
}

/// The mDNS record for `advert`, advertising `addrs`.
fn service_info(advert: &Advert, addrs: &[IpAddr]) -> ServiceInfo {
    let host = format!("mouser-{}.local.", &advert.computer.to_string()[..8]);
    ServiceInfo::new(
        SERVICE_TYPE,
        &advert.display_name,
        &host,
        addrs,
        advert.port,
        txt(advert).as_slice(),
    )
    .expect("valid mDNS record")
}

fn socket_addr(ip: &ScopedIp, port: u16) -> SocketAddr {
    match ip {
        ScopedIp::V6(v6) => {
            SocketAddr::V6(SocketAddrV6::new(*v6.addr(), port, 0, v6.scope_id().index))
        }
        other => SocketAddr::new(other.to_ip_addr(), port),
    }
}

/// Interpret a resolved mDNS record; `None` when it is not a valid Mouser record.
fn found_from(resolved: &ResolvedService) -> Option<Found> {
    let prop = |key: &str| resolved.txt_properties.get_property_val_str(key);
    let desk = match prop("desk")? {
        "" => None,
        id => Some((id.parse().ok()?, prop("dn")?.to_string())),
    };
    let port: u16 = prop("port")?.parse().ok()?;
    let mut addrs: Vec<SocketAddr> = resolved
        .addresses
        .iter()
        .map(|ip| socket_addr(ip, port))
        .collect();
    addrs.sort_by_key(|a| (a.is_ipv6(), *a));
    Some(Found {
        computer: prop("cid")?.parse().ok()?,
        display_name: resolved
            .fullname
            .strip_suffix(&format!(".{SERVICE_TYPE}"))?
            .to_string(),
        protocol: prop("v")?.parse().ok()?,
        desk,
        addrs,
    })
}

/// Desks seen on the network, with the names of their visible members;
/// `except` (the desk this computer is on) is left out.
pub fn group_by_desk(found: &[Found], except: Option<DeskId>) -> Vec<DiscoveredDesk> {
    let mut desks: BTreeMap<DeskId, DiscoveredDesk> = BTreeMap::new();
    for f in found {
        if let Some((id, name)) = &f.desk
            && Some(*id) != except
        {
            desks
                .entry(*id)
                .or_insert_with(|| DiscoveredDesk {
                    desk_id: *id,
                    name: name.clone(),
                    members: Vec::new(),
                })
                .members
                .push(f.display_name.clone());
        }
    }
    desks.into_values().collect()
}

/// The real mDNS implementation, restricted to physical LAN interfaces.
pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    interfaces: Vec<NetInterface>,
    registered: Mutex<Option<String>>,
    found: Arc<Mutex<BTreeMap<String, Found>>>,
}

impl MdnsDiscovery {
    /// Start browsing; `own` is left out of [`Discovery::found`].
    pub fn new(own: ComputerId, interfaces: Vec<NetInterface>) -> Self {
        let daemon = ServiceDaemon::new().expect("mDNS daemon starts");
        daemon
            .disable_interface(IfKind::All)
            .expect("mDNS daemon alive");
        let allowed = interfaces.clone();
        let predicate = IfPredicate::new(move |i| is_enabled(&allowed, &i.name, i.ip()));
        daemon
            .enable_interface(IfKind::Predicate(predicate))
            .expect("mDNS daemon alive");
        let events = daemon.browse(SERVICE_TYPE).expect("mDNS daemon alive");
        let found = Arc::new(Mutex::new(BTreeMap::new()));
        let seen = found.clone();
        std::thread::spawn(move || {
            while let Ok(event) = events.recv() {
                match event {
                    ServiceEvent::ServiceResolved(resolved) => {
                        if let Some(f) = found_from(&resolved).filter(|f| f.computer != own) {
                            seen.lock().unwrap().insert(resolved.fullname.clone(), f);
                        }
                    }
                    ServiceEvent::ServiceRemoved(_, fullname) => {
                        seen.lock().unwrap().remove(&fullname);
                    }
                    _ => {}
                }
            }
        });
        Self {
            daemon,
            interfaces,
            registered: Mutex::new(None),
            found,
        }
    }

    fn unregister(&self) {
        if let Some(fullname) = self.registered.lock().unwrap().take() {
            self.daemon
                .unregister(&fullname)
                .expect("mDNS daemon alive")
                .recv_timeout(CONFIRM)
                .expect("mDNS unregister confirmed");
        }
    }
}

impl Discovery for MdnsDiscovery {
    fn announce(&self, advert: &Advert) {
        self.unregister();
        let info = service_info(advert, &advertised_addrs(&self.interfaces));
        *self.registered.lock().unwrap() = Some(info.get_fullname().to_string());
        self.daemon.register(info).expect("mDNS daemon alive");
    }

    fn found(&self) -> Vec<Found> {
        self.found.lock().unwrap().values().cloned().collect()
    }

    fn stop(&self) {
        self.unregister();
        self.daemon
            .shutdown()
            .expect("mDNS daemon alive")
            .recv_timeout(CONFIRM)
            .expect("mDNS shutdown confirmed");
    }
}
