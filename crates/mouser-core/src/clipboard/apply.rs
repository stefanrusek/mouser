//! Applying a desk member's offer to this computer's clipboard (Story 6
//! scenario 1).
//!
//! Representations up to 1 MiB arrive inline and are all written at once; the
//! larger ones, and files, are fetched when the user pastes. What this
//! computer's clipboard cannot hold is dropped and the best remaining
//! representation is written (edge case: unrepresentable format).

use mouser_platform::{ClipRep, Clipboard};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{
    ClipboardKind, ClipboardPayload, ClipboardRepresentation, ControlMessage,
};

use super::{Clip, Out, decode, kind_of};

/// The latest offer from another computer.
pub struct Remote {
    pub origin: ComputerId,
    pub sequence: u64,
    /// The representations that arrived inline.
    pub held: Vec<ClipRep>,
    /// Kinds that came by reference.
    pub referenced: Vec<ClipboardKind>,
    /// Pasting still has to fetch the referenced kinds: they have neither
    /// been fetched nor been replaced by a copy made here.
    pub pending: bool,
}

impl Remote {
    /// Every kind the offer carried.
    pub fn kinds(&self) -> Vec<ClipboardKind> {
        self.held
            .iter()
            .map(kind_of)
            .chain(self.referenced.iter().copied())
            .collect()
    }
}

/// Content being fetched from its origin.
pub struct Fetching {
    pub origin: ComputerId,
    pub sequence: u64,
    /// What is here so far.
    pub have: Vec<ClipRep>,
    pub waiting: Vec<ClipboardKind>,
    /// The transfer bringing files, once the origin offered it.
    pub transfer: Option<mouser_proto::wire::TransferId>,
}

/// What the receiver keeps first when it cannot hold everything: plain text,
/// then the formats in the order most applications understand them.
const PREFERENCE: [ClipboardKind; 5] = [
    ClipboardKind::TextPlain,
    ClipboardKind::TextHtml,
    ClipboardKind::TextRtf,
    ClipboardKind::ImagePng,
    ClipboardKind::Files,
];

/// Write `reps`. If the clipboard cannot hold them all, keep the most
/// universal representations that it can hold and drop the rest.
pub fn write_best(clip: &mut dyn Clipboard, reps: Vec<ClipRep>) {
    if clip.write(reps.clone()).is_ok() {
        return;
    }
    let mut accepted = Vec::new();
    let mut last_ok = true;
    for kind in PREFERENCE {
        let Some(rep) = reps.iter().find(|rep| kind_of(rep) == kind) else {
            continue;
        };
        accepted.push(rep.clone());
        last_ok = clip.write(accepted.clone()).is_ok();
        if !last_ok {
            accepted.pop();
        }
    }
    if !last_ok && !accepted.is_empty() {
        // A refused write may have left the clipboard half-written.
        clip.write(accepted)
            .expect("the clipboard holds what it held a moment ago");
    }
}

impl Clip {
    /// A desk member copied something.
    pub fn on_offer(
        &mut self,
        clip: &mut dyn Clipboard,
        from: ComputerId,
        sequence: u64,
        representations: Vec<ClipboardRepresentation>,
    ) {
        let mut held = Vec::new();
        let mut referenced = Vec::new();
        for rep in representations {
            match rep.payload {
                ClipboardPayload::Inline(bytes) => held.push(decode(rep.kind, bytes)),
                ClipboardPayload::Reference { .. } => referenced.push(rep.kind),
            }
        }
        self.fetching = None;
        self.mine = false;
        if !held.is_empty() {
            self.write(clip, held.clone());
        }
        self.remote = Some(Remote {
            origin: from,
            sequence,
            held,
            pending: !referenced.is_empty(),
            referenced,
        });
    }

    /// Write what Mouser got from elsewhere, and tag it as not the user's copy.
    fn write(&mut self, clip: &mut dyn Clipboard, reps: Vec<ClipRep>) {
        write_best(clip, reps);
        self.watcher.wrote(clip);
    }

    /// The user pasted: fetch what was only referenced.
    pub fn paste(&mut self) -> Vec<Out> {
        let Some(remote) = &self.remote else {
            return Vec::new();
        };
        if !remote.pending || self.fetching.is_some() {
            return Vec::new();
        }
        let (kinds, have) = (remote.referenced.clone(), remote.held.clone());
        self.begin(kinds, have)
    }

    /// Ask the origin of the latest offer for `kinds`.
    pub(super) fn begin(&mut self, kinds: Vec<ClipboardKind>, have: Vec<ClipRep>) -> Vec<Out> {
        let remote = self.remote.as_ref().expect("fetching needs an offer");
        let outs = kinds
            .iter()
            .map(|kind| {
                Out::To(
                    remote.origin,
                    ControlMessage::ClipboardFetch {
                        sequence: remote.sequence,
                        kind: *kind,
                    },
                )
            })
            .collect();
        self.fetching = Some(Fetching {
            origin: remote.origin,
            sequence: remote.sequence,
            have,
            waiting: kinds,
            transfer: None,
        });
        outs
    }

    /// One fetched representation arrived.
    pub fn on_data(
        &mut self,
        clip: &mut dyn Clipboard,
        from: ComputerId,
        sequence: u64,
        kind: ClipboardKind,
        bytes: Vec<u8>,
    ) {
        let Some(fetching) = &mut self.fetching else {
            return;
        };
        if fetching.origin != from || fetching.sequence != sequence {
            return;
        }
        if !fetching.waiting.contains(&kind) {
            return;
        }
        fetching.add(kind, decode(kind, bytes));
        self.finish_if_complete(clip);
    }

    /// Everything fetched is written, as one clipboard.
    pub(super) fn finish_if_complete(&mut self, clip: &mut dyn Clipboard) {
        let complete = self.fetching.as_ref().is_some_and(|f| f.waiting.is_empty());
        if !complete {
            return;
        }
        let fetched = self.fetching.take().expect("checked above");
        self.write(clip, fetched.have.clone());
        let remote = self.remote.as_mut().expect("fetching needs an offer");
        remote.held = fetched.have;
        remote.referenced.clear();
        remote.pending = false;
    }

    /// A computer left: whatever it still owed us is gone.
    pub fn peer_gone(&mut self, peer: ComputerId) {
        if self.fetching.as_ref().is_some_and(|f| f.origin == peer) {
            self.fetching = None;
        }
        if self.remote.as_ref().is_some_and(|r| r.origin == peer) {
            self.remote = None;
        }
    }
}

impl Fetching {
    /// `rep` of `kind` arrived: it replaces what we had of that kind.
    pub fn add(&mut self, kind: ClipboardKind, rep: ClipRep) {
        self.waiting.retain(|k| *k != kind);
        self.have.retain(|r| kind_of(r) != kind);
        self.have.push(rep);
    }
}
