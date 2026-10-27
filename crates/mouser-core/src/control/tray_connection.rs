//! What the tray says about the link to the desk-mates (data-model.md
//! "Connection state for the tray").

use std::time::Instant;

use mouser_proto::control::{ConnectionState, TrayConnection};
use mouser_proto::ids::ComputerId;

use crate::engine::Core;

/// No desk, or no other member: `Alone`. Every member connected (whether or
/// not this computer is paused): `Connected`. Otherwise `Disconnected`.
/// Members the owner removed, or the desk this computer left, are no longer
/// in the member list, so they never count.
pub(crate) fn tray_connection(core: &Core, me: ComputerId, now: Instant) -> TrayConnection {
    let Some(desk) = &core.state.desk else {
        return TrayConnection::Alone;
    };
    let mut others = desk
        .state
        .members
        .iter()
        .filter(|m| m.computer_id != me)
        .peekable();
    if others.peek().is_none() {
        TrayConnection::Alone
    } else if others.all(|m| core.connection_of(m.computer_id, now) == ConnectionState::Connected) {
        TrayConnection::Connected
    } else {
        TrayConnection::Disconnected
    }
}
