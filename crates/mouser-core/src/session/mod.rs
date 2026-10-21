//! Per-connection protocol: handshake, desk authentication and the link
//! driver that moves messages between the wire and the engine.

pub mod auth;
pub mod handshake;

#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod handshake_tests;

use std::net::SocketAddr;
use std::sync::Arc;

use mouser_proto::control::{FixAction, ProblemKind};
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::wire::ControlMessage;
use tokio::sync::mpsc;

use crate::discovery::Found;
use crate::engine::{Ctx, Outgoing, Peer, Phase};
use crate::transport::{FrameReader, FrameWriter, Link, Transport};
use handshake::{PeerHello, exchange, incompatibility};

/// Drains queued messages to the wire, then closes the connection.
async fn write_loop(
    mut writer: FrameWriter,
    mut out: mpsc::UnboundedReceiver<Outgoing>,
    conn: quinn::Connection,
) {
    while let Some(Outgoing::Message(msg)) = out.recv().await {
        // A dead connection is noticed (and cleaned up) by the reader.
        let _ = writer.send(&msg).await;
    }
    writer.finish().await;
    conn.close(0u32.into(), b"done");
}

/// Removes the peer when its link ends, also when the reader task panics.
struct LinkGuard {
    ctx: Arc<Ctx>,
    peer: ComputerId,
    link_id: u64,
}

impl Drop for LinkGuard {
    fn drop(&mut self) {
        self.ctx.on_link_closed(self.peer, self.link_id);
    }
}

async fn read_loop(ctx: Arc<Ctx>, peer: ComputerId, link_id: u64, mut reader: FrameReader) {
    let _guard = LinkGuard {
        ctx: ctx.clone(),
        peer,
        link_id,
    };
    while let Some(msg) = reader.recv::<ControlMessage>().await {
        ctx.on_message(peer, msg)
            .expect("peer followed the protocol");
    }
}

async fn watch_closed(ctx: Arc<Ctx>, peer: ComputerId, link_id: u64, conn: quinn::Connection) {
    let _guard = LinkGuard { ctx, peer, link_id };
    conn.closed().await;
}

impl Ctx {
    /// Dial `addr` and exchange `Hello`s; `None` if unreachable, not the
    /// expected computer, or the peer hung up.
    pub(crate) async fn dial(
        &self,
        addr: SocketAddr,
        expected: Option<ComputerId>,
    ) -> Option<(Link, PeerHello)> {
        let conn = self.transport.connect(addr, expected).await.ok()?;
        let mut link = Link::open(conn).await;
        let hello = exchange(&mut link, self.hello()).await?;
        Some((link, hello))
    }

    /// Dial a discovered computer and take over the connection.
    pub(crate) async fn dial_found(self: Arc<Self>, found: Found) {
        let dialled = self.dial(found.addrs[0], Some(found.computer)).await;
        self.core.lock().unwrap().dialing.remove(&found.computer);
        if let Some((link, hello)) = dialled {
            self.adopt(link, hello, None);
        }
    }

    /// Finish an incoming connection, exchange `Hello`s and take it over.
    pub(crate) async fn accept_one(self: Arc<Self>, incoming: quinn::Incoming) -> Option<()> {
        let conn = Transport::complete(incoming).await.ok()?;
        let mut link = Link::accepted(conn).await?;
        let hello = exchange(&mut link, self.hello()).await?;
        self.adopt(link, hello, None);
        Some(())
    }

    /// Register a greeted connection. With `join`, we are the joiner and ask
    /// that desk's member to admit us; otherwise desk-mates prove membership.
    pub(crate) fn adopt(self: &Arc<Self>, link: Link, hello: PeerHello, join: Option<DeskId>) {
        let Link {
            conn,
            peer,
            writer,
            reader,
        } = link;
        let (out, out_rx) = mpsc::unbounded_channel();
        self.spawn(write_loop(writer, out_rx, conn.clone()));
        let link_id = self.next_link_id();
        let compatible = hello.compatible_with(self.protocol_version);
        let my_name = self.display_name();
        let incompatible_text = incompatibility(
            &my_name,
            &self.mouser_version,
            self.protocol_version,
            &hello,
        );
        {
            let mut core = self.core.lock().unwrap();
            let same_desk = core.state.desk.as_ref().map(|d| d.desk_id) == hello.desk_id
                && hello.desk_id.is_some();
            let phase = match (compatible, join.is_some()) {
                (false, _) => Phase::Incompatible,
                (true, true) => Phase::AwaitingDecision,
                (true, false) => Phase::Greeted,
            };
            let entry = Peer {
                link_id,
                hello: hello.clone(),
                conn: conn.clone(),
                out,
                phase,
                displays: Vec::new(),
                last_seen: crate::problems::now(),
            };
            if let Some(old) = core.peers.insert(peer, entry) {
                old.conn.close(0u32.into(), b"replaced");
            }
            if let Some(desk_id) = join.filter(|_| compatible) {
                core.peers[&peer].send(ControlMessage::JoinRequest {
                    desk_id,
                    device_public_key: self.identity.public_key(),
                    display_name: my_name,
                    os: crate::platform_os(self.platform.info().os),
                });
            } else if same_desk && compatible {
                self.send_auth(&core, peer);
            }
        }
        if compatible {
            self.problems
                .clear(ProblemKind::IncompatibleVersion, &peer.to_string());
            self.spawn(read_loop(self.clone(), peer, link_id, reader));
        } else {
            let (text, mine_is_older) = incompatible_text;
            self.problems.raise(
                ProblemKind::IncompatibleVersion,
                &peer.to_string(),
                peer,
                text.clone(),
                mine_is_older.then_some(FixAction::OpenUpdate),
            );
            if join.is_some() {
                self.fail_pending(&text);
            }
            self.spawn(watch_closed(self.clone(), peer, link_id, conn));
        }
        self.notify_status();
    }

    /// Forget a peer whose link ended (unless a newer link replaced it).
    pub(crate) fn on_link_closed(&self, peer: ComputerId, link_id: u64) {
        let waiting = {
            let mut core = self.core.lock().unwrap();
            if core.peers.get(&peer).is_none_or(|p| p.link_id != link_id) {
                return;
            }
            let gone = core.peers.remove(&peer).expect("checked above");
            core.requests.drop_peer(peer);
            gone.phase == Phase::AwaitingDecision
        };
        if waiting {
            self.fail_pending("The request to join the desk was not answered.");
        }
        self.notify_status();
    }

    /// Route one message from an established peer to its handler.
    pub(crate) fn on_message(
        self: &Arc<Self>,
        peer: ComputerId,
        msg: ControlMessage,
    ) -> Result<(), String> {
        if let Some(entry) = self.core.lock().unwrap().peers.get_mut(&peer) {
            entry.last_seen = crate::problems::now();
        }
        match msg {
            ControlMessage::JoinRequest {
                desk_id,
                device_public_key,
                display_name,
                os,
            } => self.on_join_request(peer, desk_id, device_public_key, display_name, os),
            ControlMessage::JoinDecision { approved, sealed } => {
                self.on_join_decision(peer, approved, sealed)
            }
            ControlMessage::DeskAuth { hmac } => self.on_desk_auth(peer, hmac),
            ControlMessage::DeskState(state) => self.on_desk_state(peer, state),
            ControlMessage::Leave { computer_id } => self.on_leave(peer, computer_id),
            ControlMessage::Remove { computer_id } => self.on_remove(peer, computer_id),
            ControlMessage::Displays { displays } => self.on_displays(peer, displays),
            other => Err(format!("unexpected message {other:?}")),
        }
    }
}
