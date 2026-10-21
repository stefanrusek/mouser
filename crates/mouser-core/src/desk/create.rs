//! Desk creation (Story 1 scenario 3, second part).

use std::sync::Arc;

use mouser_proto::control::{Desk, RpcError};
use mouser_proto::ids::DeskId;

use crate::control::invalid;
use crate::engine::Ctx;
use crate::state::{Membership, PersistedDesk};

/// Trim the name and require 1-64 characters.
pub fn validate_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    let length = trimmed.chars().count();
    if (1..=64).contains(&length) {
        Ok(trimmed.to_string())
    } else {
        Err(format!(
            "a desk name needs 1 to 64 characters, not {length}"
        ))
    }
}

/// `len` random bytes.
pub fn random<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    getrandom::fill(&mut bytes).expect("operating system randomness");
    bytes
}

impl Ctx {
    /// `desk.create`: a new desk with this computer as its only member.
    pub(crate) fn create_desk(self: &Arc<Self>, name: &str) -> Result<Desk, RpcError> {
        let name = validate_name(name).map_err(invalid)?;
        let desk = {
            let mut core = self.core.lock().unwrap();
            if core.state.membership != Membership::Unjoined {
                return Err(invalid("this computer is already on, or joining, a desk"));
            }
            let desk_id = DeskId(random());
            let key: [u8; 32] = random();
            let state = self.new_desk_state(&core);
            self.store_desk_key(&desk_id, &key);
            core.desk_key = Some(key);
            core.state.membership = Membership::Member(desk_id);
            core.state.desk = Some(PersistedDesk {
                desk_id,
                name: name.clone(),
                state: state.clone(),
            });
            self.persist(&core);
            Desk {
                desk_id,
                name,
                state,
            }
        };
        self.announce();
        self.complete_first_run();
        self.notify_status();
        Ok(desk)
    }
}
