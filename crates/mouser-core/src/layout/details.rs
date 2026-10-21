//! Display details (Story 4 scenario 2): what the arranger shows when the
//! owner points at a display.

use std::time::Instant;

use mouser_proto::control::ConnectionState;
use mouser_proto::model::Display;
use serde::Serialize;

use crate::engine::{Core, Ctx};

/// A display as `layout.get` reports it: its own fields (resolution is
/// `pixel_size`, with `scale`) and who owns it and whether they are there.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DisplayDetails {
    #[serde(flatten)]
    pub display: Display,
    pub computer_name: String,
    pub online: bool,
    /// When an offline computer was last seen.
    pub last_seen: Option<String>,
    /// The placement was made automatically and the owner has not looked at it.
    pub needs_review: bool,
}

impl Ctx {
    /// Every display on the desk, this computer's first.
    pub(crate) fn display_details(&self, core: &Core) -> Vec<DisplayDetails> {
        let me = self.me();
        let mut known = core.state.displays.clone();
        known.insert(me, self.local_displays(core));
        let now = Instant::now();
        let mut computers: Vec<_> = known.keys().copied().collect();
        computers.sort_by_key(|c| (*c != me, *c));
        computers
            .into_iter()
            .flat_map(|computer| {
                let (name, online, last_seen) = if computer == me {
                    (self.display_name_in(core), true, None)
                } else {
                    let name = core
                        .state
                        .desk
                        .as_ref()
                        .and_then(|d| d.state.members.iter().find(|m| m.computer_id == computer))
                        .map(|m| m.display_name.clone())
                        .expect("displays are kept for members only");
                    let online = core.connection_of(computer, now) == ConnectionState::Connected;
                    let last_seen = core
                        .absent
                        .get(&computer)
                        .filter(|_| !online)
                        .map(|a| a.last_seen.clone());
                    (name, online, last_seen)
                };
                known[&computer]
                    .iter()
                    .map(|display| DisplayDetails {
                        display: display.clone(),
                        computer_name: name.clone(),
                        online,
                        last_seen: last_seen.clone(),
                        needs_review: core.needs_review.contains(&display.display_id),
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
