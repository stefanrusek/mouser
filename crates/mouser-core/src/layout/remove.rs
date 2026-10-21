//! Display removal and undo (Story 4 scenario 3): a display with
//! `included = false` is not part of the layout, so the pointer never
//! crosses onto it; including it again undoes that.

use std::collections::{BTreeMap, BTreeSet};

use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId};
use serde::Deserialize;

/// A request to include or remove one display.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Inclusion {
    pub display_id: DisplayId,
    pub included: bool,
}

/// Apply `changes` to the `known` displays of every computer. A computer
/// removes its own displays only: `removed` (the keys of `me`'s removed
/// displays) follows. Requests that change nothing are no-ops.
pub fn apply(
    me: ComputerId,
    known: &mut BTreeMap<ComputerId, Vec<Display>>,
    removed: &mut BTreeSet<String>,
    changes: &[Inclusion],
) -> Result<(), String> {
    for change in changes {
        let id = &change.display_id;
        let Some(display) = known
            .get_mut(&id.computer)
            .and_then(|ds| ds.iter_mut().find(|d| &d.display_id == id))
        else {
            return Err(format!("{} is not a display on this desk", id.key));
        };
        if display.included == change.included {
            continue;
        }
        if id.computer != me {
            return Err(format!(
                "{} belongs to another computer; remove it on that computer",
                id.key
            ));
        }
        display.included = change.included;
        if change.included {
            removed.remove(&id.key);
        } else {
            removed.insert(id.key.clone());
        }
    }
    Ok(())
}
