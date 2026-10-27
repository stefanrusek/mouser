//! Collecting the desk's logs (data-model "Collection states"). The
//! requester asks only the computers that can answer, parks one waiter for
//! each, and records every desk computer: with its logs, or with the reason
//! it has none. Nothing waits on a timer: a part is complete when its stream
//! finishes, and a link that is lost ends the wait for that computer.

use std::sync::Arc;
use std::time::Instant;

use mouser_proto::control::{BundleReason, ConnectionState};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::Os;
use mouser_proto::wire::{LogsPart, V2_SINCE, logs_stream_id};
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;

use super::bundle::Computer;
use crate::engine::{Core, Ctx, Phase};
use crate::lock::LockExt;
use crate::release::Version;

/// What collecting does about one desk computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Plan {
    /// Send it a `LogsRequest`.
    Ask,
    /// It cannot be asked, for this reason.
    Skip(BundleReason),
}

/// A desk computer other than this one.
#[derive(Debug, Clone)]
pub(crate) struct Target {
    pub id: ComputerId,
    pub name: String,
    pub os: Os,
    pub version: String,
    pub connection: ConnectionState,
    pub plan: Plan,
}

/// A stream waited for: its id, the computer that answers on it, and where
/// to hand it and the length of its header.
pub(crate) type Waiter = (
    [u8; 16],
    ComputerId,
    oneshot::Sender<(quinn::RecvStream, usize)>,
);

impl Ctx {
    /// The other computers of the desk and what to do about each: only those
    /// connected and at least 0.3.0 can be asked (an older one panics at a
    /// message it does not know).
    pub(crate) fn logs_targets(&self, core: &Core) -> Vec<Target> {
        let since = Version::parse(V2_SINCE).expect("a version");
        let now = Instant::now();
        let Some(desk) = &core.state.desk else {
            return Vec::new();
        };
        desk.state
            .members
            .iter()
            .filter(|m| m.computer_id != self.me())
            .map(|member| {
                let peer = core.peers.get(&member.computer_id);
                let version = peer.map_or_else(
                    || member.mouser_version.clone(),
                    |p| p.hello.mouser_version.clone(),
                );
                let plan = if !Version::parse(&version).is_ok_and(|v| v >= since) {
                    Plan::Skip(BundleReason::OlderVersion)
                } else if peer.is_some_and(|p| p.phase == Phase::Authed) {
                    Plan::Ask
                } else {
                    Plan::Skip(BundleReason::Offline)
                };
                Target {
                    id: member.computer_id,
                    name: member.display_name.clone(),
                    os: member.os,
                    version,
                    connection: core.connection_of(member.computer_id, now),
                    plan,
                }
            })
            .collect()
    }

    /// Hand `stream`, which answers a `LogsRequest`, to the waiter for
    /// `peer`'s answer; if nobody waits for it, it is given back.
    pub(crate) fn deliver_logs(
        &self,
        id: &[u8; 16],
        peer: ComputerId,
        header_len: usize,
        stream: quinn::RecvStream,
    ) -> Option<quinn::RecvStream> {
        let mut waiters = self.logs_waiters.guard();
        let Some(at) = waiters
            .iter()
            .position(|(waiting, from, _)| waiting == id && *from == peer)
        else {
            return Some(stream);
        };
        let (_, _, tx) = waiters.remove(at);
        // A collection that gave up has no use for the answer.
        let _ = tx.send((stream, header_len));
        None
    }

    /// Stop waiting for answers from `peer`, whose connection ended.
    pub(crate) fn drop_logs_waiters(&self, peer: ComputerId) {
        self.logs_waiters
            .guard()
            .retain(|(_, from, _)| *from != peer);
    }

    /// Every computer of the desk, this one first, with its logs or the
    /// reason there are none.
    pub(crate) async fn collect_logs(self: &Arc<Self>) -> Vec<Computer> {
        let mut request = [0u8; 16];
        getrandom::fill(&mut request).expect("the system has randomness");
        let id = logs_stream_id(&request);
        let (targets, waiting) = {
            let core = self.core.guard();
            let targets = self.logs_targets(&core);
            let mut waiting = Vec::new();
            for target in targets.iter().filter(|t| t.plan == Plan::Ask) {
                let (tx, rx) = oneshot::channel();
                self.logs_waiters.guard().push((id, target.id, tx));
                core.peers[&target.id]
                    .send(mouser_proto::wire::ControlMessage::LogsRequest { request });
                waiting.push(rx);
            }
            (targets, waiting)
        };
        let (part, files) = self.logs_part();
        let mut computers = vec![from_part(part, files, ConnectionState::SelfComputer)];
        let mut waiting = waiting.into_iter();
        for target in targets {
            let answer = match target.plan {
                Plan::Ask => {
                    read_answer(waiting.next().expect("one waiter per computer asked")).await
                }
                Plan::Skip(_) => None,
            };
            computers.push(match answer {
                Some((part, files)) => from_part(part, files, target.connection),
                None => unreached(&target),
            });
        }
        computers
    }
}

/// What a computer sent: its header and the bytes of each file it names, or
/// `None` when the link was lost before the whole answer arrived.
async fn read_answer(
    waiter: oneshot::Receiver<(quinn::RecvStream, usize)>,
) -> Option<(LogsPart, Vec<Vec<u8>>)> {
    // The sender is dropped when the link is lost.
    let (stream, header_len) = waiter.await.ok()?;
    let mut stream = stream;
    let mut header = Vec::new();
    (&mut stream)
        .take(header_len as u64)
        .read_to_end(&mut header)
        .await
        .ok()?;
    let part: LogsPart = postcard::from_bytes(&header).ok()?;
    let mut files = Vec::new();
    for (_, len) in &part.files {
        let mut bytes = Vec::new();
        (&mut stream)
            .take(*len)
            .read_to_end(&mut bytes)
            .await
            .ok()?;
        files.push(bytes);
    }
    Some((part, files))
}

/// A computer that sent its part.
fn from_part(part: LogsPart, files: Vec<Vec<u8>>, connection: ConnectionState) -> Computer {
    Computer {
        id: part.card.id,
        name: part.card.name,
        os: part.card.os,
        os_version: Some(part.card.os_version),
        version: part.card.version,
        mode: Some(part.card.mode),
        connection,
        included: part.logging || connection == ConnectionState::SelfComputer,
        reason: (!part.logging && connection != ConnectionState::SelfComputer)
            .then_some(BundleReason::LoggingOff),
        config_toml: Some(part.config_toml),
        files: part
            .files
            .into_iter()
            .map(|(name, _)| name)
            .zip(files)
            .collect(),
    }
}

/// A computer that sent nothing: its card is what this computer knows.
fn unreached(target: &Target) -> Computer {
    Computer {
        id: target.id,
        name: target.name.clone(),
        os: target.os,
        os_version: None,
        version: target.version.clone(),
        mode: None,
        connection: target.connection,
        included: false,
        reason: Some(match target.plan {
            Plan::Skip(reason) => reason,
            // Asked, and the link was lost first.
            Plan::Ask => BundleReason::Offline,
        }),
        config_toml: None,
        files: Vec::new(),
    }
}
