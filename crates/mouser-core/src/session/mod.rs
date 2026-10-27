//! Per-connection protocol: handshake, desk authentication and the link
//! driver that moves messages between the wire and the engine.

pub mod auth;
pub mod close_reason;
pub mod handshake;
pub mod liveness;
pub mod redial;

#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod close_reason_tests;
#[cfg(test)]
pub(crate) mod fault_tests;
#[cfg(test)]
mod handshake_tests;
#[cfg(test)]
mod liveness_tests;
#[cfg(test)]
mod redial_tests;
#[cfg(test)]
mod v2_placeholder_tests;

use crate::lock::LockExt;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mouser_proto::control::{FixAction, ProblemKind};
use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::wire::ControlMessage;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::discovery::Found;
use crate::engine::{Ctx, Outgoing, Peer, Phase};
use crate::faults::{self, Faulted, RecoveryAction, run_unit};
use crate::logging::events;
use crate::transport::{FrameReader, FrameWriter, Link, Transport};
use close_reason::{CloseReason, LinkLossReason};
use handshake::{PeerHello, exchange, incompatibility};

/// The longest one attempt to reach a computer may take (research R2: a
/// re-dial is at most a second); a failed attempt is followed by another on
/// the next reconcile tick.
pub const DIAL_ATTEMPT: Duration = Duration::from_secs(1);

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
    conn.close(0u32.into(), CloseReason::Done.bytes());
}

/// Removes the peer when its link ends, also when the reader task panics.
struct LinkGuard {
    ctx: Arc<Ctx>,
    peer: ComputerId,
    link_id: u64,
}

impl Drop for LinkGuard {
    fn drop(&mut self) {
        let close = || self.ctx.on_link_closed(self.peer, self.link_id);
        if std::thread::panicking() {
            // Unwinding from a panic: the engine's state may be poisoned by
            // it, and a second panic out of this destructor would abort the
            // process and hide the first. Clean up if that can be done.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(close));
        } else {
            close();
        }
    }
}

async fn read_loop(
    ctx: Arc<Ctx>,
    peer: ComputerId,
    link_id: u64,
    conn: quinn::Connection,
    mut reader: FrameReader,
) {
    let _guard = LinkGuard {
        ctx: ctx.clone(),
        peer,
        link_id,
    };
    while let Some(msg) = reader.recv::<ControlMessage>().await {
        // A link that was replaced by a newer connection to the same
        // computer no longer speaks for it.
        if !ctx.is_current(peer, link_id) {
            break;
        }
        match run_unit(&ctx.events, "read_loop", || ctx.on_message(peer, msg)) {
            Ok(Ok(())) => {}
            // The peer broke the protocol: this link ends, the desk heals
            // by the usual re-dial.
            Ok(Err(_violation)) => {
                ctx.close_link(peer, link_id, &conn, CloseReason::Protocol);
                break;
            }
            Err(Faulted) => {
                ctx.fault_link("read_loop", peer, link_id, &conn);
                break;
            }
        }
    }
    // A frame that could not be decoded closed the connection (the reader
    // says so); the link was lost for that.
    if reader.undecodable() {
        ctx.note_closing(peer, link_id, CloseReason::Protocol.loss());
    }
}

/// Applies the motion datagrams of `peer` until the connection ends.
async fn motion_loop(ctx: Arc<Ctx>, peer: ComputerId, link_id: u64, conn: quinn::Connection) {
    while let Ok(bytes) = conn.read_datagram().await {
        let handled = run_unit(&ctx.events, "motion_loop", || {
            let motion = postcard::from_bytes(&bytes).expect("well-formed datagram");
            ctx.crossing.guard().on_motion(peer, motion);
        });
        if handled.is_err() {
            ctx.fault_link("motion_loop", peer, link_id, &conn);
            break;
        }
    }
}

/// Receives the files that desk-mates stream to us until the connection ends.
async fn file_loop(ctx: Arc<Ctx>, peer: ComputerId, conn: quinn::Connection) {
    while let Ok(mut stream) = conn.accept_uni().await {
        // A stream cut short by a lost connection is a transfer that did not finish.
        let Ok((id, index)) = crate::transfer::streams::read_header(&mut stream).await else {
            continue;
        };
        let Some(stream) = ctx.deliver_release(&id, stream) else {
            continue;
        };
        // For the logs stream the "index" is the length of its header.
        let Some(stream) = ctx.deliver_logs(&id.0, peer, index, stream) else {
            continue;
        };
        if ctx.transfer_ids.contains(&id) {
            ctx.spawn(ctx.clone().receive_dragged(id, index, stream));
        } else if let Ok(Some(paths)) = ctx.clip_inbox.receive(id, index, stream).await {
            // No clipboard task means this platform shares no clipboard.
            let _ = ctx
                .clip
                .send(crate::clipboard::Command::FilesArrived(paths));
        }
    }
}

/// Takes a computer out of `dialing` when its dial attempt ends, however it
/// ends (finished, panicked, or dropped as the engine stops), so the computer
/// can be dialled again.
struct DialingGuard {
    ctx: Arc<Ctx>,
    peer: ComputerId,
}

impl Drop for DialingGuard {
    fn drop(&mut self) {
        self.ctx.core.guard().dialing.remove(&self.peer);
    }
}

async fn watch_closed(ctx: Arc<Ctx>, peer: ComputerId, link_id: u64, conn: quinn::Connection) {
    let _guard = LinkGuard { ctx, peer, link_id };
    conn.closed().await;
}

impl Ctx {
    /// A unit of work on this link faulted: control comes back here, nothing
    /// stays held, and the link is closed with reason `fault` so that the
    /// redial brings it back.
    fn fault_link(
        &self,
        at: &'static str,
        peer: ComputerId,
        link_id: u64,
        conn: &quinn::Connection,
    ) {
        self.crossing.guard().recover_after_fault();
        self.close_link(peer, link_id, conn, CloseReason::Fault);
        faults::recovered(&self.log, at, RecoveryAction::ClosedLink);
    }

    /// The link `link_id` to `peer` was lost for `reason`, which this
    /// computer knows better than the connection's error says.
    pub(crate) fn note_closing(&self, peer: ComputerId, link_id: u64, reason: LinkLossReason) {
        if let Some(entry) = self
            .core
            .guard()
            .peers
            .get_mut(&peer)
            .filter(|p| p.link_id == link_id)
        {
            entry.closing = Some(reason);
        }
    }

    /// Close the connection of the link `link_id` to `peer` for `why`.
    fn close_link(
        &self,
        peer: ComputerId,
        link_id: u64,
        conn: &quinn::Connection,
        why: CloseReason,
    ) {
        self.note_closing(peer, link_id, why.loss());
        conn.close(0u32.into(), why.bytes());
    }

    /// Dial `addr` and exchange `Hello`s; `None` if unreachable, not the
    /// expected computer, or the peer hung up.
    pub(crate) async fn dial(
        &self,
        addr: SocketAddr,
        expected: Option<ComputerId>,
    ) -> Option<(Link, PeerHello)> {
        let conn = self.transport.connect(addr, expected).await.ok()?;
        let mut link = Link::open(conn).await?;
        let hello = exchange(&mut link, self.hello()).await?;
        Some((link, hello))
    }

    /// Dial every address a computer is found at at once, so one that is
    /// gone (an unplugged cable, a stopped VPN) does not hold up the others;
    /// the first computer to answer wins. Gives up after [`DIAL_ATTEMPT`].
    pub(crate) async fn dial_any(
        self: &Arc<Self>,
        addrs: &[SocketAddr],
        expected: ComputerId,
    ) -> Option<(Link, PeerHello)> {
        let mut attempts = JoinSet::new();
        for addr in addrs {
            let (ctx, addr) = (self.clone(), *addr);
            attempts.spawn(async move { ctx.dial(addr, Some(expected)).await });
        }
        tokio::time::timeout(DIAL_ATTEMPT, async {
            while let Some(done) = attempts.join_next().await {
                if let Some(answered) = done.expect("dial attempts do not panic") {
                    return Some(answered);
                }
            }
            None
        })
        .await
        .ok()
        .flatten()
    }

    /// Dial a discovered computer and take over the connection.
    pub(crate) async fn dial_found(self: Arc<Self>, found: Found) {
        let dialled = {
            let _dialing = DialingGuard {
                ctx: self.clone(),
                peer: found.computer,
            };
            self.dial_any(&found.addrs, found.computer).await
        };
        if let Some((link, hello)) = dialled {
            self.adopt(link, hello, None, true);
        }
    }

    /// Finish an incoming connection, exchange `Hello`s and take it over.
    pub(crate) async fn accept_one(self: Arc<Self>, incoming: quinn::Incoming) -> Option<()> {
        let conn = Transport::complete(incoming).await.ok()?;
        let mut link = Link::accepted(conn).await?;
        let hello = exchange(&mut link, self.hello()).await?;
        self.adopt(link, hello, None, false);
        Some(())
    }

    /// Register a greeted connection, which we `dialed` or accepted. With
    /// `join`, we are the joiner and ask that desk's member to admit us;
    /// otherwise desk-mates prove membership.
    pub(crate) fn adopt(
        self: &Arc<Self>,
        link: Link,
        hello: PeerHello,
        join: Option<DeskId>,
        dialed: bool,
    ) {
        let Link {
            conn,
            peer,
            writer,
            mut reader,
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
            let mut core = self.core.guard();
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
                dialed,
                closing: None,
                phase,
                displays: Vec::new(),
                status: None,
                paused: false,
                last_seen: crate::problems::now(),
            };
            if let Some(old) = core.peers.insert(peer, entry) {
                old.conn.close(0u32.into(), CloseReason::Replaced.bytes());
                self.log.write(events::peer_disconnected(
                    peer,
                    CloseReason::Replaced.loss(),
                ));
                // The old connection's session is over: whatever it held
                // down is released and control comes back here, and until
                // the new connection proves membership the computer is
                // reconnecting, not offline.
                if old.phase == Phase::Authed {
                    core.mark_absent(peer, old.last_seen, Instant::now());
                }
                self.refresh_map(&core);
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
            let log = self.log.clone();
            reader.on_unknown(move |variant| log.write(events::wire_unknown(peer, variant)));
            self.spawn(read_loop(self.clone(), peer, link_id, conn.clone(), reader));
            self.spawn(motion_loop(self.clone(), peer, link_id, conn.clone()));
            self.spawn(file_loop(self.clone(), peer, conn.clone()));
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

    /// Whether `link_id` is the current connection to `peer`.
    pub(crate) fn is_current(&self, peer: ComputerId, link_id: u64) -> bool {
        self.core
            .guard()
            .peers
            .get(&peer)
            .is_some_and(|p| p.link_id == link_id)
    }

    /// Forget a peer whose link ended (unless a newer link replaced it).
    pub(crate) fn on_link_closed(&self, peer: ComputerId, link_id: u64) {
        let (waiting, name) = {
            let mut core = self.core.guard();
            if core.peers.get(&peer).is_none_or(|p| p.link_id != link_id) {
                return;
            }
            let gone = core.peers.remove(&peer).expect("checked above");
            // A link the peer finished may end before its close arrives: the
            // peer then left.
            let reason = gone.closing.unwrap_or_else(|| {
                gone.conn
                    .close_reason()
                    .map_or(LinkLossReason::PeerLeft, |error| LinkLossReason::of(&error))
            });
            self.log.write(events::peer_disconnected(peer, reason));
            if gone.phase != Phase::Departed && core.is_member(peer) {
                core.mark_absent(peer, gone.last_seen, Instant::now());
            }
            core.requests.drop_peer(peer);
            self.refresh_map(&core);
            (
                gone.phase == Phase::AwaitingDecision,
                gone.hello.display_name,
            )
        };
        self.problems
            .clear(ProblemKind::SecureInput, &peer.to_string());
        self.drop_release_waiters(peer);
        self.drop_logs_waiters(peer);
        // No clipboard task means this platform shares no clipboard.
        let _ = self.clip.send(crate::clipboard::Command::PeerGone(peer));
        // Likewise for dragged files.
        let _ = self
            .xfer
            .send(crate::transfer::engine::Command::PeerGone(peer, name));
        if waiting {
            self.fail_pending("The request to join the desk was not answered.");
        }
        self.notify_status();
    }

    /// Hand input from `peer` to the crossing state: only desk members may
    /// send it.
    fn input(
        &self,
        peer: ComputerId,
        apply: impl FnOnce(&mut crate::crossing::Crossing),
    ) -> Result<(), String> {
        if !Self::authed(&self.core.guard(), peer) {
            return Err("input from a computer that is not on the desk".into());
        }
        apply(&mut self.crossing.guard());
        Ok(())
    }

    /// Route one message from an established peer to its handler.
    pub(crate) fn on_message(
        self: &Arc<Self>,
        peer: ComputerId,
        msg: ControlMessage,
    ) -> Result<(), String> {
        if let Some(entry) = self.core.guard().peers.get_mut(&peer) {
            if entry.phase == Phase::Departed {
                return Ok(());
            }
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
            ControlMessage::Status(status) => self.on_peer_status(peer, status),
            ControlMessage::Paused { paused } => self.on_peer_paused(peer, paused),
            message @ (ControlMessage::ClipboardOffer { .. }
            | ControlMessage::ClipboardFetch { .. }
            | ControlMessage::ClipboardData { .. }) => self.clipboard_message(peer, message),
            message @ (ControlMessage::TransferOffer { .. }
            | ControlMessage::TransferAccept { .. }
            | ControlMessage::TransferReject { .. }
            | ControlMessage::TransferConflict { .. }
            | ControlMessage::TransferResolve { .. }
            | ControlMessage::TransferDone { .. }) => self.transfer_message(peer, message),
            ControlMessage::ReleaseAvailable { manifest_json } => {
                self.on_release_available(peer, &manifest_json)
            }
            ControlMessage::ReleaseFetch { version, asset } => {
                self.on_release_fetch(peer, version, asset)
            }
            ControlMessage::Enter {
                display_id,
                x,
                y,
                seq_base,
                ..
            } => self
                .crossing
                .guard()
                .on_enter(peer, display_id, x, y, seq_base),
            ControlMessage::Key {
                seq, usage, down, ..
            } => self.input(peer, |c| c.on_key(peer, seq, usage, down)),
            ControlMessage::Button {
                seq, button, down, ..
            } => self.input(peer, |c| c.on_button(peer, seq, button, down)),
            ControlMessage::Scroll { seq, dx, dy } => {
                self.input(peer, |c| c.on_scroll(peer, seq, dx, dy))
            }
            ControlMessage::QuickJumpShow { letters } => {
                self.input(peer, |c| c.on_quickjump_show(peer, letters))
            }
            ControlMessage::SecureAttention => self.input(peer, |c| c.on_secure_attention(peer)),
            ControlMessage::QuickJumpHide => self.input(peer, |c| c.on_quickjump_hide()),
            ControlMessage::ReleaseAll { .. } => self.input(peer, |c| c.on_release_all(peer)),
            ControlMessage::Panic => self.input(peer, |c| c.on_panic(peer)),
            ControlMessage::LogsRequest { request } => self.on_logs_request(peer, request),
            // v2: replaced in T205
            ControlMessage::DeskSettings { .. } => Ok(()),
            // v2: replaced in T226
            ControlMessage::RestartElevated => Ok(()),
            other => Err(format!("unexpected message {other:?}")),
        }
    }
}
