//! Network-change handling (Story 3 scenarios 2 to 4, research R2 and R3).
//!
//! `Platform::watch_network` fires when an interface appears, disappears or
//! changes address: waking from sleep, Wi-Fi to Ethernet, a new DHCP lease, a
//! VPN, virtual machine or container network starting or stopping. Mouser
//! needs no setting for any of them:
//!
//! 1. the interfaces are reclassified and discovery is announced again, on
//!    physical LAN interfaces only, so a virtual adapter's addresses are
//!    never advertised;
//! 2. if the physical LAN addresses changed, the QUIC endpoint is rebound
//!    (`Endpoint::rebind_abstract`) onto new handles of its wildcard-bound
//!    sockets,
//!    which migrates every connection this computer dialled to the new
//!    address with one round trip, on the configured port;
//! 3. and connections this computer accepted are dialled again (`redial`).

use crate::lock::LockExt;
use std::collections::BTreeSet;
use std::net::{IpAddr, SocketAddr};

use mouser_platform::{InterfaceKind, NetInterface};
use std::sync::Arc;

use super::Transport;
use crate::engine::Ctx;

impl Transport {
    /// Tell the endpoint its path changed, keeping every connection. The
    /// sockets stay the ones bound to the configured port on the wildcard
    /// addresses (`0.0.0.0` and `[::]`), so the operating system sends from
    /// whatever address the computer has now, and join-by-address and
    /// firewall rules on that port keep working. The endpoint is handed
    /// fresh handles of those sockets: that is what makes every connection
    /// probe its path (QUIC migration, research R2). Returns the address.
    pub fn rebind(&self) -> SocketAddr {
        self.endpoint
            .rebind_abstract(self.sockets.quinn())
            .expect("endpoint can rebind");
        self.local_addr()
    }
}

/// The addresses of the physical LAN interfaces: what a connection's path
/// depends on. Virtual adapters come and go without moving it.
pub fn physical_addrs(interfaces: &[NetInterface]) -> BTreeSet<IpAddr> {
    interfaces
        .iter()
        .filter(|i| i.kind == InterfaceKind::PhysicalLan)
        .flat_map(|i| i.addrs.iter().copied())
        .collect()
}

impl Ctx {
    /// The network changed under us. Whenever a VPN, virtual machine or
    /// container network starts or stops, or the computer wakes, discovery is
    /// told so it announces on the physical LAN only. Only if the physical
    /// LAN addresses themselves changed is the path of every connection
    /// affected: then the endpoint moves, and connections we accepted are
    /// dialled again. A change that leaves the physical addresses alone
    /// leaves working connections alone (a session must not end because a
    /// VPN started).
    pub(crate) fn on_network_change(self: &Arc<Self>) {
        let interfaces = self.platform.interfaces();
        let physical = physical_addrs(&interfaces);
        let moved =
            std::mem::replace(&mut self.core.guard().physical, physical.clone()) != physical;
        self.discovery.interfaces_changed(interfaces);
        if moved {
            self.transport.rebind();
        }
        // The advert carries the endpoint's port; announce the new addresses.
        self.announce();
        if moved {
            self.redial_accepted();
        }
    }
}
