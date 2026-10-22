//! Watching the local clipboard and offering what is copied (Story 6).
//!
//! `change_count` is polled; an increase means the user copied something, and
//! the content goes to every desk member as a `ClipboardOffer` with a growing
//! sequence. What Mouser writes itself is tagged by re-reading the counter
//! after the write, so it is never offered back (no echo).

use std::path::PathBuf;

use mouser_platform::{ClipRep, Clipboard};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{
    ClipboardKind, ClipboardPayload, ClipboardRepresentation, ControlMessage, TransferItem,
};

use super::{Clip, INLINE_LIMIT, Out, encode, kind_of};
use crate::transfer::streams::manifest;

pub struct Watcher {
    /// The counter as last seen, including after our own writes.
    seen: u64,
    sequence: u64,
}

impl Watcher {
    /// Start watching; what is on the clipboard now is not shared.
    pub fn new(clip: &mut dyn Clipboard) -> Self {
        Self {
            seen: clip.change_count(),
            sequence: 0,
        }
    }

    /// Mouser just wrote the clipboard: that change is not the user's.
    pub fn wrote(&mut self, clip: &mut dyn Clipboard) {
        self.seen = clip.change_count();
    }

    /// The new content and its sequence, if the user copied since last time.
    pub fn poll(&mut self, clip: &mut dyn Clipboard) -> Option<(u64, Vec<ClipRep>)> {
        let count = clip.change_count();
        if count == self.seen {
            return None;
        }
        self.seen = count;
        let reps = clip.read().expect("clipboard readable");
        if reps.is_empty() {
            return None;
        }
        self.sequence += 1;
        Some((self.sequence, reps))
    }
}

/// What this computer offered last, kept to serve fetches.
pub struct Served {
    pub sequence: u64,
    pub reps: Vec<ClipRep>,
    /// The copied files and folders, with where they are here.
    pub files: Vec<(TransferItem, PathBuf)>,
}

/// The offer for `reps`: small representations inline, large ones and files
/// by reference, files with their manifest.
pub fn offer(sequence: u64, reps: &[ClipRep]) -> (ControlMessage, Served) {
    let mut files = Vec::new();
    let mut representations = Vec::new();
    for rep in reps {
        let payload = if let ClipRep::Files(paths) = rep {
            files = manifest(paths);
            ClipboardPayload::Reference {
                size: files.iter().map(|(item, _)| item.size).sum(),
            }
        } else {
            let bytes = encode(rep).expect("everything but files has bytes");
            if bytes.len() <= INLINE_LIMIT {
                ClipboardPayload::Inline(bytes)
            } else {
                ClipboardPayload::Reference {
                    size: bytes.len() as u64,
                }
            }
        };
        representations.push(ClipboardRepresentation {
            kind: kind_of(rep),
            payload,
        });
    }
    let message = ControlMessage::ClipboardOffer {
        sequence,
        representations,
        file_manifest: files.iter().map(|(item, _)| item.clone()).collect(),
    };
    let served = Served {
        sequence,
        reps: reps.to_vec(),
        files,
    };
    (message, served)
}

impl Clip {
    /// Look at the clipboard: new content is offered to the whole desk.
    pub fn poll(&mut self, clip: &mut dyn Clipboard) -> Vec<Out> {
        let Some((sequence, reps)) = self.watcher.poll(clip) else {
            return Vec::new();
        };
        // What the user copied here replaces whatever was still to be fetched.
        self.fetching = None;
        if let Some(remote) = &mut self.remote {
            remote.pending = false;
        }
        let (message, served) = offer(sequence, &reps);
        self.served = Some(served);
        self.mine = true;
        if self.paused {
            return Vec::new();
        }
        vec![Out::All(message)]
    }

    /// Offer again what the user copied here, if the clipboard still holds
    /// it: to `peer`, or to everyone, after a pause is over.
    pub fn offer_mine(&mut self, peer: Option<ComputerId>) -> Vec<Out> {
        let served = match (&self.served, self.mine, self.paused) {
            (Some(served), true, false) => served,
            _ => return Vec::new(),
        };
        let (message, _) = offer(served.sequence, &served.reps);
        vec![peer.map_or(Out::All(message.clone()), |p| Out::To(p, message))]
    }

    /// A desk member asks for part of what we offered.
    pub fn on_fetch(&mut self, from: ComputerId, sequence: u64, kind: ClipboardKind) -> Vec<Out> {
        let Some(served) = &self.served else {
            return Vec::new();
        };
        if served.sequence != sequence {
            return Vec::new();
        }
        if kind == ClipboardKind::Files {
            return self.offer_files(from);
        }
        served
            .reps
            .iter()
            .find(|rep| kind_of(rep) == kind)
            .and_then(encode)
            .map(|bytes| {
                Out::To(
                    from,
                    ControlMessage::ClipboardData {
                        sequence,
                        kind,
                        bytes,
                    },
                )
            })
            .into_iter()
            .collect()
    }
}
