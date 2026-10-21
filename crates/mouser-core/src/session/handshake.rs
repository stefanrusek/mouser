//! The `Hello` handshake and protocol-version check (Story 1 scenario 8).

use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::Os;
use mouser_proto::wire::{ControlMessage, is_compatible};

use crate::transport::Link;

/// What the other computer said about itself in its `Hello`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerHello {
    pub computer_id: ComputerId,
    pub mouser_version: String,
    pub protocol_version: u16,
    pub display_name: String,
    pub os: Os,
    pub desk_id: Option<DeskId>,
}

impl PeerHello {
    pub fn compatible_with(&self, my_protocol: u16) -> bool {
        is_compatible(self.protocol_version, my_protocol)
    }
}

/// Send `mine` (a `ControlMessage::Hello`) and read the peer's, the first
/// frame in each direction. `None` if the peer hung up first.
pub async fn exchange(link: &mut Link, mine: ControlMessage) -> Option<PeerHello> {
    link.writer.send(&mine).await.ok()?;
    let ControlMessage::Hello {
        protocol_version,
        mouser_version,
        computer_id,
        display_name,
        os,
        desk_id,
    } = link.reader.recv().await?
    else {
        panic!("first frame must be Hello");
    };
    assert_eq!(
        computer_id, link.peer,
        "Hello does not match the certificate"
    );
    Some(PeerHello {
        computer_id,
        mouser_version,
        protocol_version,
        display_name,
        os,
        desk_id,
    })
}

/// The plain-language message for an incompatible peer, and whether this
/// computer is the older one (the one to update).
pub fn incompatibility(
    my_name: &str,
    my_version: &str,
    my_protocol: u16,
    peer: &PeerHello,
) -> (String, bool) {
    let mine_is_older = my_protocol < peer.protocol_version;
    let to_update = if mine_is_older {
        my_name
    } else {
        &peer.display_name
    };
    let text = format!(
        "{} runs Mouser {} and {my_name} runs Mouser {my_version}, and those versions cannot work together. Update {to_update}.",
        peer.display_name, peer.mouser_version,
    );
    (text, mine_is_older)
}
