//! `SimNet`: sanctioned in-process UDP relay network (decision D2).
//!
//! Each node is an application (for example a quinn endpoint) with a real
//! loopback UDP socket. A node never talks to a peer's real address; it dials
//! the address returned by [`SimNode::route_to`], a relay socket that forwards
//! to the peer, so the relay can delay, drop, re-address or silence traffic.
//! The peer sees packets arrive from the relay's per-link source address, and
//! replies to it. Must be used inside a tokio runtime; relay tasks live as long
//! as the runtime.

use crate::report;
use crate::virtual_platform::VirtualPlatform;
use mouser_platform::NetInterface;
use std::io;
use std::net::{SocketAddr, UdpSocket as StdUdpSocket};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

struct NodeState {
    name: String,
    real: SocketAddr,
    blackhole: bool,
    asleep: bool,
    delay: Duration,
    interfaces: Vec<NetInterface>,
    platform: Option<VirtualPlatform>,
}

/// One direction of dialing: `from` dials `front`, packets go out of the
/// current `back` socket to `to`'s real address.
struct Link {
    from: usize,
    to: usize,
    front: Arc<UdpSocket>,
    back: Mutex<Arc<UdpSocket>>,
    back_task: Mutex<Option<JoinHandle<()>>>,
    peer: Mutex<Option<SocketAddr>>,
}

#[derive(Default)]
struct NetState {
    nodes: Vec<NodeState>,
    links: Vec<Arc<Link>>,
    dropped: u64,
}

/// The simulated network. Cloning gives another handle to the same network.
#[derive(Clone)]
pub struct SimNet {
    state: Arc<Mutex<NetState>>,
}

/// A node on a [`SimNet`]. Cloning gives another handle to the same node.
#[derive(Clone)]
pub struct SimNode {
    net: SimNet,
    id: usize,
}

fn bind_loopback() -> Arc<UdpSocket> {
    let std_socket = StdUdpSocket::bind("127.0.0.1:0").unwrap();
    std_socket.set_nonblocking(true).unwrap();
    Arc::new(UdpSocket::from_std(std_socket).unwrap())
}

/// Where relay datagrams come from; the real socket, or a script in tests.
trait Receive {
    async fn receive(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)>;
}

impl Receive for UdpSocket {
    async fn receive(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.recv_from(buf).await
    }
}

/// Receive the next datagram, or `None` when the source is broken. Windows
/// reports a send to a closed loopback port as `ConnectionReset` on the next
/// receive; like quinn, ignore it: it says nothing about the relay itself.
async fn recv_relayed(source: &impl Receive, buf: &mut [u8]) -> Option<(usize, SocketAddr)> {
    loop {
        match source.receive(buf).await {
            Ok(received) => return Some(received),
            Err(e) if e.kind() == io::ErrorKind::ConnectionReset => continue,
            Err(_) => return None,
        }
    }
}

impl SimNet {
    pub fn new() -> Self {
        report::note("SimNet");
        SimNet {
            state: Arc::new(Mutex::new(NetState::default())),
        }
    }

    /// Register a node whose application listens on `real`.
    pub fn add_node(&self, name: &str, real: SocketAddr) -> SimNode {
        let mut s = self.lock();
        s.nodes.push(NodeState {
            name: name.to_string(),
            real,
            blackhole: false,
            asleep: false,
            delay: Duration::ZERO,
            interfaces: Vec::new(),
            platform: None,
        });
        SimNode {
            net: self.clone(),
            id: s.nodes.len() - 1,
        }
    }

    /// Packets discarded so far because a node was black-holed or asleep.
    pub fn dropped_packets(&self) -> u64 {
        self.lock().dropped
    }

    fn lock(&self) -> MutexGuard<'_, NetState> {
        self.state.lock().unwrap()
    }

    /// Extra latency for a packet between two nodes, or `None` if it must be
    /// dropped (counted).
    fn verdict(&self, from: usize, to: usize) -> Option<Duration> {
        let mut s = self.lock();
        let (a, b) = (&s.nodes[from], &s.nodes[to]);
        if a.blackhole || a.asleep || b.blackhole || b.asleep {
            s.dropped += 1;
            return None;
        }
        Some(a.delay + b.delay)
    }

    /// Apply the current conditions to one packet and send it via `socket`.
    async fn forward(
        &self,
        from: usize,
        to: usize,
        data: Vec<u8>,
        socket: Arc<UdpSocket>,
        dest: SocketAddr,
    ) {
        let Some(delay) = self.verdict(from, to) else {
            return;
        };
        if delay.is_zero() {
            let _ = socket.send_to(&data, dest).await;
            return;
        }
        let net = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            // Conditions may have changed while the packet was in flight.
            if net.verdict(from, to).is_some() {
                let _ = socket.send_to(&data, dest).await;
            }
        });
    }

    fn spawn_back_task(&self, link: &Arc<Link>, back: Arc<UdpSocket>) -> JoinHandle<()> {
        let (net, link) = (self.clone(), link.clone());
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            while let Some((n, _)) = recv_relayed(&*back, &mut buf).await {
                let peer = *link.peer.lock().unwrap();
                if let Some(peer) = peer {
                    net.forward(
                        link.to,
                        link.from,
                        buf[..n].to_vec(),
                        link.front.clone(),
                        peer,
                    )
                    .await;
                }
            }
        })
    }
}

impl Default for SimNet {
    fn default() -> Self {
        Self::new()
    }
}

impl SimNode {
    pub fn name(&self) -> String {
        self.node(|n| n.name.clone())
    }

    /// The address this node's application currently listens on.
    pub fn addr(&self) -> SocketAddr {
        self.node(|n| n.real)
    }

    fn node<T>(&self, f: impl FnOnce(&mut NodeState) -> T) -> T {
        f(&mut self.net.lock().nodes[self.id])
    }

    /// The address this node's application must dial to reach `to`. Stable for
    /// the life of the network, also across address changes of either node.
    pub fn route_to(&self, to: &SimNode) -> SocketAddr {
        if let Some(link) = self
            .net
            .lock()
            .links
            .iter()
            .find(|l| l.from == self.id && l.to == to.id)
        {
            return link.front.local_addr().unwrap();
        }
        let (front, back) = (bind_loopback(), bind_loopback());
        let link = Arc::new(Link {
            from: self.id,
            to: to.id,
            front: front.clone(),
            back: Mutex::new(back.clone()),
            back_task: Mutex::new(None),
            peer: Mutex::new(None),
        });
        *link.back_task.lock().unwrap() = Some(self.net.spawn_back_task(&link, back));
        let (net, task_link) = (self.net.clone(), link.clone());
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            while let Some((n, source)) = recv_relayed(&*task_link.front, &mut buf).await {
                *task_link.peer.lock().unwrap() = Some(source);
                let dest = net.lock().nodes[task_link.to].real;
                let back = task_link.back.lock().unwrap().clone();
                net.forward(task_link.from, task_link.to, buf[..n].to_vec(), back, dest)
                    .await;
            }
        });
        self.net.lock().links.push(link);
        front.local_addr().unwrap()
    }

    /// Add one-way latency to every packet to or from this node.
    pub fn set_delay(&self, delay: Duration) {
        self.node(|n| n.delay = delay);
    }

    /// Drop every packet to and from this node, in both directions.
    pub fn set_blackhole(&self, on: bool) {
        self.node(|n| n.blackhole = on);
    }

    /// The node's application moved to `new_real` (for example after
    /// `Endpoint::rebind`). Peers now receive its packets from a different
    /// source address, as after a real network change; the old source address
    /// stops working.
    pub fn change_address(&self, new_real: SocketAddr) {
        self.node(|n| n.real = new_real);
        let links: Vec<Arc<Link>> = self
            .net
            .lock()
            .links
            .iter()
            .filter(|l| l.from == self.id)
            .cloned()
            .collect();
        for link in links {
            let back = bind_loopback();
            *link.back.lock().unwrap() = back.clone();
            let task = self.net.spawn_back_task(&link, back);
            if let Some(old) = link.back_task.lock().unwrap().replace(task) {
                old.abort();
            }
        }
    }

    /// The computer is suspended: all traffic to and from it is lost.
    pub fn sleep(&self) {
        self.node(|n| n.asleep = true);
    }

    /// The computer resumes; its platform (if bound) sees a network change.
    pub fn wake(&self) {
        let platform = self.node(|n| {
            n.asleep = false;
            n.platform.clone()
        });
        if let Some(platform) = platform {
            platform.trigger_network_change();
        }
    }

    pub fn is_asleep(&self) -> bool {
        self.node(|n| n.asleep)
    }

    /// Attach the node's `VirtualPlatform`: interface changes made through this
    /// node are applied to it (firing its `watch_network`), and `wake` fires it.
    /// The node's interface list starts as the platform's current list.
    pub fn bind_platform(&self, platform: &VirtualPlatform) {
        use mouser_platform::Platform;
        self.node(|n| {
            n.interfaces = platform.interfaces();
            n.platform = Some(platform.clone());
        });
    }

    pub fn interfaces(&self) -> Vec<NetInterface> {
        self.node(|n| n.interfaces.clone())
    }

    /// Add (or replace, by name) a virtual or physical interface.
    pub fn add_interface(&self, interface: NetInterface) {
        self.update_interfaces(|list| {
            list.retain(|i| i.name != interface.name);
            list.push(interface);
        });
    }

    /// Remove the interface called `name`; returns whether it existed.
    pub fn remove_interface(&self, name: &str) -> bool {
        let mut removed = false;
        self.update_interfaces(|list| {
            let before = list.len();
            list.retain(|i| i.name != name);
            removed = list.len() != before;
        });
        removed
    }

    fn update_interfaces(&self, change: impl FnOnce(&mut Vec<NetInterface>)) {
        let (interfaces, platform) = self.node(|n| {
            change(&mut n.interfaces);
            (n.interfaces.clone(), n.platform.clone())
        });
        if let Some(platform) = platform {
            platform.set_interfaces(interfaces);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    struct Script(Mutex<VecDeque<io::Result<(usize, SocketAddr)>>>);

    impl Receive for Script {
        async fn receive(&self, _buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
            self.0.lock().unwrap().pop_front().unwrap()
        }
    }

    fn script(items: Vec<io::Result<(usize, SocketAddr)>>) -> Script {
        Script(Mutex::new(items.into()))
    }

    #[tokio::test]
    async fn connection_reset_is_skipped() {
        let from: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let source = script(vec![
            Err(io::ErrorKind::ConnectionReset.into()),
            Err(io::ErrorKind::ConnectionReset.into()),
            Ok((3, from)),
        ]);
        let got = recv_relayed(&source, &mut [0u8; 8]).await;
        assert_eq!(got, Some((3, from)));
    }

    #[tokio::test]
    async fn any_other_error_ends_the_relay() {
        let source = script(vec![Err(io::ErrorKind::BrokenPipe.into())]);
        assert_eq!(recv_relayed(&source, &mut [0u8; 8]).await, None);
    }
}
