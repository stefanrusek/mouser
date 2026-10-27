//! Local state persistence: `state.json` holds desk membership, layout and
//! epoch. The desk key lives only in the `SecretStore` (`desk_key/<desk_id>`).

use std::path::Path;

use std::collections::{BTreeMap, BTreeSet};

use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::{DeskState, Display};
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

/// Which `DeskSettings` this computer holds: the higher `(epoch, author)`
/// wins, as for the desk state. Nothing was ever changed at `default()`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsVersion {
    pub epoch: u64,
    pub author: Option<ComputerId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalState {
    pub membership: Membership,
    pub desk: Option<PersistedDesk>,
    /// The last displays every desk member (this computer included) reported:
    /// the layout stores only placements, so sizes live here.
    #[serde(default)]
    pub displays: BTreeMap<ComputerId, Vec<Display>>,
    /// Keys of this computer's displays the owner removed from the layout.
    #[serde(default)]
    pub removed: BTreeSet<String>,
    /// The version of the desk-shared settings held.
    #[serde(default)]
    pub settings: SettingsVersion,
}

impl Default for LocalState {
    fn default() -> Self {
        Self {
            membership: Membership::Unjoined,
            desk: None,
            displays: BTreeMap::new(),
            removed: BTreeSet::new(),
            settings: SettingsVersion::default(),
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

    /// Write `path`; a file that cannot be written is the caller's to keep
    /// going without.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).expect("state serializes");
        std::fs::write(path, json)
    }
}

/// Secret-store name of a desk's key.
pub fn desk_key_name(desk: &DeskId) -> String {
    format!("desk_key/{desk}")
}

#[cfg(test)]
mod tests;
