//! The join request / one-click approval flow (Story 1 scenarios 2-3, FR-013).
//!
//! The joiner dials a member and sends `JoinRequest`; the member's owner
//! decides with `join.decide`; an approval sends `JoinDecision` carrying the
//! desk key and desk state. That payload ("sealed" in the wire contract) rides
//! inside the QUIC connection, whose TLS keys are bound to the two device keys.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use mouser_proto::control::{
    ComputerRef, DeskJoinParams, DeskRef, JoinPending, Notification,
    PendingJoin as PendingJoinItem, PendingState, ProblemKind, RpcError, Topic,
};
use mouser_proto::ids::{ComputerId, DeskId, DevicePublicKey};
use mouser_proto::model::{DeskState, Member, Os};
use mouser_proto::wire::ControlMessage;
use serde::{Deserialize, Serialize};

use crate::control::invalid;
use crate::desk::{allowlist, membership};
use crate::engine::{Ctx, PendingJoin, Phase};
use crate::session::handshake::PeerHello;
use crate::state::{Membership, PersistedDesk};
use crate::transport::Link;

/// The payload of an approving `JoinDecision` (and of a desk-key rotation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sealed {
    pub key: [u8; 32],
    pub name: String,
    pub state: DeskState,
}

pub fn seal(sealed: &Sealed) -> Vec<u8> {
    postcard::to_stdvec(sealed).expect("sealed desk serializes")
}

pub fn open(bytes: &[u8]) -> Sealed {
    postcard::from_bytes(bytes).expect("well-formed sealed desk")
}

/// A join request waiting for the owner's decision.
#[derive(Debug, Clone)]
pub(crate) struct PendingRequest {
    pub peer: ComputerId,
    pub name: String,
    pub os: Os,
    pub key: DevicePublicKey,
    pub desk: DeskId,
}

/// Join requests waiting for a decision, by request id.
#[derive(Default)]
pub(crate) struct Requests {
    next: u64,
    items: BTreeMap<u64, PendingRequest>,
}

impl Requests {
    pub fn add(&mut self, request: PendingRequest) -> u64 {
        self.next += 1;
        self.items.insert(self.next, request);
        self.next
    }

    pub fn take(&mut self, id: u64) -> Option<PendingRequest> {
        self.items.remove(&id)
    }

    pub fn drop_peer(&mut self, peer: ComputerId) {
        self.items.retain(|_, r| r.peer != peer);
    }

    pub fn iter(&self) -> impl Iterator<Item = (u64, &PendingRequest)> {
        self.items.iter().map(|(id, r)| (*id, r))
    }
}

impl Ctx {
    // ---- member side ------------------------------------------------------

    /// A computer asks to join our desk.
    pub(crate) fn on_join_request(
        self: &Arc<Self>,
        peer: ComputerId,
        desk_id: DeskId,
        key: DevicePublicKey,
        name: String,
        os: Os,
    ) -> Result<(), String> {
        let mut core = self.core.lock().unwrap();
        let my_desk = core.state.desk.as_ref().map(|d| d.desk_id);
        let entry = core.peers.get(&peer).expect("peer registered");
        if entry.phase != Phase::Greeted
            || my_desk != Some(desk_id)
            || ComputerId::from_public_key(&key) != peer
        {
            return Err(format!("invalid JoinRequest from {peer}"));
        }
        let ip = entry.conn.remote_address().ip();
        let desk = core.state.desk.as_ref().expect("on a desk");
        if !allowlist::permits(&desk.state.allowlist, ip) {
            entry.send(ControlMessage::JoinDecision {
                approved: false,
                sealed: None,
            });
            entry.close();
            drop(core);
            self.problems.raise(
                ProblemKind::JoinRefused,
                &peer.to_string(),
                peer,
                format!("{name} ({ip}) was refused because its address is not on the allowlist."),
                None,
            );
            return Ok(());
        }
        let desk_ref = DeskRef {
            id: desk_id,
            name: desk.name.clone(),
        };
        let request_id = core.requests.add(PendingRequest {
            peer,
            name: name.clone(),
            os,
            key,
            desk: desk_id,
        });
        drop(core);
        let item = PendingJoinItem {
            request_id,
            computer: ComputerRef { id: peer, name },
            desk: desk_ref,
        };
        // No subscribers is fine: `join.pending` lists the request.
        let _ = self.notify.send(Notification {
            jsonrpc: "2.0".into(),
            method: Topic::JoinRequest,
            params: serde_json::to_value(item).expect("request serializes"),
        });
        Ok(())
    }

    /// `join.pending`.
    pub(crate) fn pending_joins(&self) -> Vec<PendingJoinItem> {
        let core = self.core.lock().unwrap();
        let desk_name = core
            .state
            .desk
            .as_ref()
            .map(|d| d.name.clone())
            .unwrap_or_default();
        core.requests
            .iter()
            .map(|(request_id, r)| PendingJoinItem {
                request_id,
                computer: ComputerRef {
                    id: r.peer,
                    name: r.name.clone(),
                },
                desk: DeskRef {
                    id: r.desk,
                    name: desk_name.clone(),
                },
            })
            .collect()
    }

    /// `join.decide`: the owner's one-click answer.
    pub(crate) fn decide(self: &Arc<Self>, request_id: u64, approve: bool) -> Result<(), RpcError> {
        let mut core = self.core.lock().unwrap();
        let request = core
            .requests
            .take(request_id)
            .ok_or_else(|| invalid("there is no such join request"))?;
        let peer = core
            .peers
            .get(&request.peer)
            .expect("requests are dropped with their peer");
        if !approve {
            peer.send(ControlMessage::JoinDecision {
                approved: false,
                sealed: None,
            });
            peer.close();
            return Ok(());
        }
        let joiner = Member {
            computer_id: request.peer,
            device_public_key: request.key,
            display_name: request.name,
            os: request.os,
            mouser_version: peer.hello.mouser_version.clone(),
            joined_at: crate::problems::now(),
        };
        let key = core.desk_key.expect("desk key stored");
        let desk = core.state.desk.as_mut().expect("on a desk");
        let state = membership::with_member(&desk.state, joiner, self.me());
        desk.state = state.clone();
        let sealed = seal(&Sealed {
            key,
            name: desk.name.clone(),
            state: state.clone(),
        });
        core.peers[&request.peer].send(ControlMessage::JoinDecision {
            approved: true,
            sealed: Some(sealed),
        });
        self.send_auth(&core, request.peer);
        for (_, other) in core.peers.iter().filter(|(_, p)| p.phase == Phase::Authed) {
            other.send(ControlMessage::DeskState(state.clone()));
        }
        self.persist(&core);
        drop(core);
        self.notify_status();
        Ok(())
    }

    // ---- joiner side ------------------------------------------------------

    /// `desk.join` / `desk.move`.
    pub(crate) async fn join(
        self: &Arc<Self>,
        params: DeskJoinParams,
        moving: bool,
    ) -> Result<JoinPending, RpcError> {
        {
            let core = self.core.lock().unwrap();
            let refusal = match (&core.state.membership, moving) {
                (Membership::Pending(_), _) => {
                    Some("a join request is already waiting for an answer")
                }
                (Membership::Member(_), false) => {
                    Some("leave the current desk first, or move to the new one")
                }
                (Membership::Unjoined, true) => Some("this computer is not on a desk to move from"),
                _ => None,
            };
            if let Some(reason) = refusal {
                return Err(invalid(reason));
            }
        }
        match params {
            DeskJoinParams::ById { desk_id } => {
                let found = self
                    .discovery
                    .found()
                    .into_iter()
                    .find(|f| f.desk.as_ref().map(|d| d.0) == Some(desk_id))
                    .ok_or_else(|| invalid("that desk was not found on the network"))?;
                let name = found.desk.map(|d| d.1);
                self.join_via(found.addrs[0], Some(found.computer), Some(desk_id), name)
                    .await
            }
            DeskJoinParams::ByAddress { address } => {
                let addr = crate::desk::manual::resolve(&address)
                    .await
                    .ok_or_else(|| {
                        invalid(format!("{address} is not an address Mouser can use"))
                    })?;
                self.join_via(addr, None, None, None).await
            }
        }
    }

    /// Dial a member and ask to join its desk. `want` is the desk the owner
    /// chose, when known.
    async fn join_via(
        self: &Arc<Self>,
        addr: SocketAddr,
        expected: Option<ComputerId>,
        want: Option<DeskId>,
        name: Option<String>,
    ) -> Result<JoinPending, RpcError> {
        let (link, hello) = self
            .dial(addr, expected)
            .await
            .ok_or_else(|| invalid(format!("could not reach {addr}")))?;
        let desk_id = hello
            .desk_id
            .ok_or_else(|| invalid("that computer is not on a desk"))?;
        if want.is_some_and(|w| w != desk_id) {
            return Err(invalid("that computer is no longer on the desk you chose"));
        }
        self.begin_join(link, hello, desk_id, name);
        Ok(JoinPending {
            state: PendingState::Pending,
        })
    }

    fn begin_join(
        self: &Arc<Self>,
        link: Link,
        hello: PeerHello,
        desk_id: DeskId,
        name: Option<String>,
    ) {
        {
            let mut core = self.core.lock().unwrap();
            core.state.membership = Membership::Pending(desk_id);
            core.pending = Some(PendingJoin { name });
            self.persist(&core);
        }
        self.adopt(link, hello, Some(desk_id), true);
    }

    /// The join did not complete: back to what we were before, and say so.
    pub(crate) fn fail_pending(&self, message: &str) {
        let desk_id = {
            let mut core = self.core.lock().unwrap();
            let Membership::Pending(desk_id) = core.state.membership else {
                return;
            };
            core.state.membership = match &core.state.desk {
                Some(desk) => Membership::Member(desk.desk_id),
                None => Membership::Unjoined,
            };
            core.pending = None;
            self.persist(&core);
            desk_id
        };
        self.problems.raise(
            ProblemKind::JoinRefused,
            &desk_id.to_string(),
            self.me(),
            message.to_string(),
            None,
        );
        self.notify_status();
    }

    /// The decision on our `JoinRequest`, or a desk-key rotation.
    pub(crate) fn on_join_decision(
        self: &Arc<Self>,
        peer: ComputerId,
        approved: bool,
        sealed: Option<Vec<u8>>,
    ) -> Result<(), String> {
        let phase = self.core.lock().unwrap().peers.get(&peer).map(|p| p.phase);
        match (phase, approved) {
            (Some(Phase::AwaitingDecision), false) => {
                let label = self.pending_label();
                self.fail_pending(&format!("This computer was not admitted to {label}."));
                self.core.lock().unwrap().peers[&peer].close();
                Ok(())
            }
            (Some(Phase::AwaitingDecision), true) => {
                self.joined(peer, open(&sealed.expect("approval carries the desk")));
                Ok(())
            }
            (Some(Phase::Authed), true) => {
                self.rekeyed(open(&sealed.expect("rotation carries the desk")));
                Ok(())
            }
            _ => Err("unexpected JoinDecision".into()),
        }
    }

    fn pending_label(&self) -> String {
        let core = self.core.lock().unwrap();
        core.pending
            .as_ref()
            .and_then(|p| p.name.clone())
            .unwrap_or_else(|| "that desk".into())
    }

    /// We were admitted: become a member (leaving the previous desk if moving).
    fn joined(self: &Arc<Self>, peer: ComputerId, sealed: Sealed) {
        let desk_id = {
            let mut core = self.core.lock().unwrap();
            self.say_goodbye(&mut core);
            if let Some(old) = core.state.desk.take() {
                self.delete_desk_key(&old.desk_id);
            }
            let desk_id = core.peers[&peer]
                .hello
                .desk_id
                .expect("an approving member is on a desk");
            self.store_desk_key(&desk_id, &sealed.key);
            core.desk_key = Some(sealed.key);
            core.state.membership = Membership::Member(desk_id);
            core.state.desk = Some(PersistedDesk {
                desk_id,
                name: sealed.name,
                state: sealed.state,
            });
            core.pending = None;
            self.persist(&core);
            core.peers.get_mut(&peer).expect("peer registered").phase = Phase::Greeted;
            self.send_auth(&core, peer);
            self.sync_layout(&mut core);
            self.persist(&core);
            self.refresh_map(&core);
            desk_id
        };
        self.problems
            .clear(ProblemKind::JoinRefused, &desk_id.to_string());
        self.announce();
        self.complete_first_run();
        self.notify_status();
    }

    /// A member rotated the desk key and sent the new desk state.
    fn rekeyed(self: &Arc<Self>, sealed: Sealed) {
        let mut core = self.core.lock().unwrap();
        let desk = core.state.desk.as_mut().expect("authed peers imply a desk");
        if sealed.state.supersedes(&desk.state) {
            let old = desk.state.layout.clone();
            desk.state = sealed.state.clone();
            let id = desk.desk_id;
            core.desk_key = Some(sealed.key);
            self.store_desk_key(&id, &sealed.key);
            self.layout_changed(&mut core, &old, &sealed.state);
            drop(core);
            self.notify_status();
        }
    }
}
