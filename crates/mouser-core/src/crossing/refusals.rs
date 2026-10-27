//! Runs of refused input (Story 2 scenario 4). The operating system can
//! refuse hundreds of inputs in a row; the log says when a run starts, with
//! the application in front, and how long it was when it ends: with the next
//! input of that kind the system accepts, or when control changes.

use mouser_platform::Error;
use mouser_proto::ids::ComputerId;

use super::Crossing;
use crate::logging::events::{self, InputKind};

/// The refusals of one kind since the last input of that kind was accepted.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Run {
    count: u32,
    /// The controller the refused input came from, if any.
    peer: Option<ComputerId>,
}

/// One run at most per kind of input, in the order of [`InputKind`].
#[derive(Debug, Default)]
pub(crate) struct Refusals {
    runs: [Option<Run>; 6],
}

impl Crossing {
    /// The system refused an input of `kind` from `peer`.
    pub(crate) fn refused(&mut self, kind: InputKind, peer: Option<ComputerId>) {
        let slot = &mut self.refusals.runs[kind as usize];
        if let Some(run) = slot {
            run.count += 1;
            return;
        }
        *slot = Some(Run { count: 1, peer });
        let app = self.platform.foreground_app();
        self.log
            .write(events::input_refused(peer, kind, app.as_ref()));
    }

    /// The system accepted an input of `kind`: a run of refusals is over.
    pub(super) fn accepted(&mut self, kind: InputKind) {
        if let Some(run) = self.refusals.runs[kind as usize].take() {
            self.log
                .write(events::input_refused_run_ended(run.peer, kind, run.count));
        }
    }

    /// Control changed: every run of refusals is over.
    pub(super) fn end_refusal_runs(&mut self) {
        for kind in [
            InputKind::Key,
            InputKind::Button,
            InputKind::Motion,
            InputKind::Scroll,
            InputKind::Grab,
            InputKind::Warp,
        ] {
            self.accepted(kind);
        }
    }

    /// Account for what the system did with an input of `kind` from `peer`.
    pub(crate) fn tally<T>(
        &mut self,
        kind: InputKind,
        peer: Option<ComputerId>,
        result: &mouser_platform::Result<T>,
    ) {
        if result.is_ok() {
            self.accepted(kind);
        } else if matches!(result, Err(Error::Os(_))) {
            self.refused(kind, peer);
        }
    }
}
