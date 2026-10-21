//! Local state persistence: `state.json` holds desk membership, layout and
//! epoch. The desk key lives only in the `SecretStore` (`desk_key/<desk_id>`).

use std::path::Path;

use mouser_proto::ids::DeskId;
use mouser_proto::model::DeskState;
use serde::{Deserialize, Serialize};

/// File name inside the config directory.
pub const FILE: &str = "state.json";

/// `Unjoined`, `Pending(desk)` (join requested) or `Member(desk)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Membership {
    Unjoined,
    Pending(DeskId),
    Member(DeskId),
}

/// The desk this computer belongs to (also kept while a move is pending).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedDesk {
    pub desk_id: DeskId,
    pub name: String,
    pub state: DeskState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalState {
    pub membership: Membership,
    pub desk: Option<PersistedDesk>,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            membership: Membership::Unjoined,
            desk: None,
        }
    }
}

impl LocalState {
    /// Read `path`; a missing file is a computer that has never joined.
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).expect("state.json is valid"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => panic!("cannot read {}: {e}", path.display()),
        }
    }

    pub fn save(&self, path: &Path) {
        let json = serde_json::to_vec_pretty(self).expect("state serializes");
        std::fs::write(path, json).expect("state.json writable");
    }
}

/// Secret-store name of a desk's key.
pub fn desk_key_name(desk: &DeskId) -> String {
    format!("desk_key/{desk}")
}

#[cfg(test)]
mod tests;
