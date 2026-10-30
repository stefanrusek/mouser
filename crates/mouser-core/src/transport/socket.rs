//! The endpoint's sockets: an IPv4 socket and an IPv6-only socket on one
//! port, handed to quinn as one socket.
//!
//! Not one dual-stack `[::]` socket: macOS checks an IPv6 socket's port
//! against IPv6 sockets only. An ephemeral `[::]:0` is handed ports that IPv4
//! sockets hold, and an explicit `[::]:p` binds although another socket holds
//! `0.0.0.0:p` (Windows lets the two share a port too). Every IPv4 datagram to
//! the port then goes to the IPv4 socket, so the endpoint never hears an IPv4
//! peer and each handshake times out. With IPv4 on a socket of its own the
//! kernel checks the port against IPv4 sockets and delivers IPv4 to us
//! (foundational notes, "IPv4 through a dual-stack socket").

use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, UdpSocket};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};

use quinn::udp::{RecvMeta, Transmit};
use quinn::{AsyncUdpSocket, Runtime, TokioRuntime, UdpPoller};
use tokio::sync::watch;

/// The bound sockets: IPv4, and IPv6 on the same port where it can be had.
pub(super) struct Sockets {
    v4: UdpSocket,
    v6: Option<UdpSocket>,
    /// How many quinn sockets (and their pollers) are alive; see [`Held`].
    held: Arc<watch::Sender<usize>>,
}

impl Sockets {
    /// Bind `v4:port` and, where it can be bound, `v6` on the same port; with
    /// port 0 the IPv4 socket picks it. Without IPv6 (or with its port taken)
    /// the endpoint is IPv4 only. If IPv4 cannot be bound, the address and
    /// the operating system's reason.
    pub(super) fn bind(v4: Ipv4Addr, v6: Option<Ipv6Addr>, port: u16) -> Result<Self, String> {
        Self::bind_with(v4, v6, port, super::udp_socket)
    }

    pub(super) fn bind_with(
        v4: Ipv4Addr,
        v6: Option<Ipv6Addr>,
        requested: u16,
        bind: impl Fn(SocketAddr) -> io::Result<UdpSocket>,
    ) -> Result<Self, String> {
        loop {
            let addr = SocketAddr::from((v4, requested));
            let ipv4 = bind(addr).map_err(|e| format!("{addr}: {e}"))?;
            let port = ipv4.local_addr().expect("bound socket").port();
            match v6.map(|ip| bind(SocketAddr::from((ip, port)))).transpose() {
                Ok(ipv6) => return Ok(Self::new(ipv4, ipv6)),
                // The port the IPv4 socket was given is taken on IPv6: another.
                Err(e) if requested == 0 && e.kind() == io::ErrorKind::AddrInUse => {}
                Err(_) => return Ok(Self::new(ipv4, None)),
            }
        }
    }

    fn new(v4: UdpSocket, v6: Option<UdpSocket>) -> Self {
        Self {
            v4,
            v6,
            held: Arc::new(watch::channel(0).0),
        }
    }

    /// Resolves once every quinn socket made by [`quinn`](Self::quinn) (and
    /// its pollers) has been dropped, that is once the last duplicated handle
    /// of the sockets is closed. Taken before the owner is dropped.
    pub(super) fn released(&self) -> impl Future<Output = ()> + use<> {
        let mut held = self.held.subscribe();
        async move {
            held.wait_for(|n| *n == 0)
                .await
                .expect("the count is kept until the last handle is gone");
        }
    }

    /// Fresh handles of the sockets as one quinn socket (each handle is a
    /// new socket to quinn, which is what makes a rebind probe every path).
    pub(super) fn quinn(&self) -> Arc<dyn AsyncUdpSocket> {
        let wrap = |s: &UdpSocket| {
            let handle = s.try_clone().expect("socket handle can be duplicated");
            TokioRuntime
                .wrap_udp_socket(handle)
                .expect("socket joins the runtime")
        };
        let inner: Arc<dyn AsyncUdpSocket> = match &self.v6 {
            None => wrap(&self.v4),
            Some(v6) => Arc::new(Pair {
                v4: wrap(&self.v4),
                v6: wrap(v6),
                v6_first: AtomicBool::new(false),
            }),
        };
        self.held.send_modify(|n| *n += 1);
        Arc::new(Counted {
            inner,
            _held: Arc::new(Held(self.held.clone())),
        })
    }
}

/// Counts as one while alive, in [`Sockets`]' count of quinn sockets.
#[derive(Debug)]
struct Held(Arc<watch::Sender<usize>>);

impl Drop for Held {
    fn drop(&mut self) {
        self.0.send_modify(|n| *n -= 1);
    }
}

/// A quinn socket that is counted while it, or any poller made from it, is
/// alive. `_held` is declared last so it goes after what it guards: the
/// count reaches zero only once the sockets are really closed.
#[derive(Debug)]
struct Counted {
    inner: Arc<dyn AsyncUdpSocket>,
    _held: Arc<Held>,
}

impl AsyncUdpSocket for Counted {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(CountedPoller {
            inner: self.inner.clone().create_io_poller(),
            _held: self._held.clone(),
        })
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        self.inner.try_send(transmit)
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        self.inner.poll_recv(cx, bufs, meta)
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.inner.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.inner.max_transmit_segments()
    }

    fn max_receive_segments(&self) -> usize {
        self.inner.max_receive_segments()
    }

    fn may_fragment(&self) -> bool {
        self.inner.may_fragment()
    }
}

/// A poller that keeps its socket counted.
#[derive(Debug)]
struct CountedPoller {
    inner: Pin<Box<dyn UdpPoller>>,
    _held: Arc<Held>,
}

impl UdpPoller for CountedPoller {
    fn poll_writable(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        self.inner.as_mut().poll_writable(cx)
    }
}

/// An IPv4 and an IPv6 socket as one dual-stack socket: IPv4-mapped
/// destinations go out on the IPv4 one, and what it receives is reported
/// with IPv4-mapped addresses.
#[derive(Debug)]
struct Pair {
    v4: Arc<dyn AsyncUdpSocket>,
    v6: Arc<dyn AsyncUdpSocket>,
    /// Which socket is read first, alternated so neither starves the other.
    v6_first: AtomicBool,
}

impl AsyncUdpSocket for Pair {
    fn create_io_poller(self: Arc<Self>) -> Pin<Box<dyn UdpPoller>> {
        Box::pin(BothWritable(
            self.v4.clone().create_io_poller(),
            self.v6.clone().create_io_poller(),
        ))
    }

    fn try_send(&self, transmit: &Transmit) -> io::Result<()> {
        let (socket, destination) = match transmit.destination.ip().to_canonical() {
            IpAddr::V4(ip) => (
                &self.v4,
                SocketAddr::from((ip, transmit.destination.port())),
            ),
            IpAddr::V6(_) => (&self.v6, transmit.destination),
        };
        socket.try_send(&Transmit {
            destination,
            src_ip: transmit.src_ip.map(|ip| ip.to_canonical()),
            ..transmit.clone()
        })
    }

    fn poll_recv(
        &self,
        cx: &mut Context,
        bufs: &mut [io::IoSliceMut<'_>],
        meta: &mut [RecvMeta],
    ) -> Poll<io::Result<usize>> {
        let v6_first = self.v6_first.fetch_xor(true, Ordering::Relaxed);
        let order = if v6_first {
            [&self.v6, &self.v4]
        } else {
            [&self.v4, &self.v6]
        };
        for socket in order {
            if let Poll::Ready(received) = socket.poll_recv(cx, bufs, meta) {
                return Poll::Ready(received.inspect(|n| meta[..*n].iter_mut().for_each(as_ipv6)));
            }
        }
        Poll::Pending
    }

    fn local_addr(&self) -> io::Result<SocketAddr> {
        self.v6.local_addr()
    }

    fn max_transmit_segments(&self) -> usize {
        self.v4
            .max_transmit_segments()
            .min(self.v6.max_transmit_segments())
    }

    fn max_receive_segments(&self) -> usize {
        self.v4
            .max_receive_segments()
            .max(self.v6.max_receive_segments())
    }

    fn may_fragment(&self) -> bool {
        self.v4.may_fragment() || self.v6.may_fragment()
    }
}

/// A received datagram's addresses as a dual-stack socket reports them.
fn as_ipv6(meta: &mut RecvMeta) {
    meta.addr = SocketAddr::V6(match meta.addr {
        SocketAddr::V4(a) => SocketAddrV6::new(a.ip().to_ipv6_mapped(), a.port(), 0, 0),
        SocketAddr::V6(a) => a,
    });
    meta.dst_ip = meta.dst_ip.map(|ip| match ip {
        IpAddr::V4(ip) => IpAddr::V6(ip.to_ipv6_mapped()),
        ip => ip,
    });
}

/// Writable once both sockets are: quinn waits on this when a send would
/// block, without saying which socket it was for.
#[derive(Debug)]
struct BothWritable(Pin<Box<dyn UdpPoller>>, Pin<Box<dyn UdpPoller>>);

impl UdpPoller for BothWritable {
    fn poll_writable(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        std::task::ready!(self.0.as_mut().poll_writable(cx))?;
        self.1.as_mut().poll_writable(cx)
    }
}
