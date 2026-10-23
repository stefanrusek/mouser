//! Watching the local clipboard and offering what is copied (Story 6).
//!
//! `change_count` is polled; an increase means the user copied something, and
//! the content goes to every desk member as a `ClipboardOffer` with a growing
//! sequence. What Mouser writes itself is tagged by re-reading the counter
//! after the write, so it is never offered back (no echo).
//!
//! The counter alone is not enough: another clipboard-sharing tool (a virtual
//! machine's shared clipboard, say) can copy what Mouser wrote straight back,
//! and Windows advances the counter when it renders a format Mouser offered.
//! So the content Mouser last wrote or offered is kept too, and a change that
//! brings that same content back is not a copy (no ping-pong between tools).

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
    /// What Mouser last put on the desk's clipboards: written here or offered.
    last: Vec<ClipRep>,
}

/// Text as a comparison sees it: line endings and trailing NULs, which
/// clipboards and the tools copying between them change, do not count.
fn normalised(text: &str) -> String {
    text.trim_end_matches('\0').replace("\r\n", "\n")
}

/// Whether `a` and `b` hold the same content: they share a kind, and every
/// kind both hold is equal.
pub fn same_content(a: &[ClipRep], b: &[ClipRep]) -> bool {
    let equal = |x: &ClipRep, y: &ClipRep| match (x, y) {
        (ClipRep::Text(x), ClipRep::Text(y)) | (ClipRep::Html(x), ClipRep::Html(y)) => {
            normalised(x) == normalised(y)
        }
        _ => x == y,
    };
    let shared: Vec<_> = a
        .iter()
        .filter_map(|x| b.iter().find(|y| kind_of(x) == kind_of(y)).map(|y| (x, y)))
        .collect();
    !shared.is_empty() && shared.into_iter().all(|(x, y)| equal(x, y))
}

impl Watcher {
    /// Start watching; what is on the clipboard now is not shared.
    pub fn new(clip: &mut dyn Clipboard) -> Self {
        Self {
            seen: clip.change_count(),
            sequence: 0,
            last: Vec::new(),
        }
    }

    /// Mouser just wrote `reps` to the clipboard: that change is not the
    /// user's, and neither is that content coming back.
    pub fn wrote(&mut self, clip: &mut dyn Clipboard, reps: &[ClipRep]) {
        self.seen = clip.change_count();
        self.last = reps.to_vec();
    }

    /// The new content and its sequence, if the user copied since last time.
    pub fn poll(&mut self, clip: &mut dyn Clipboard) -> Option<(u64, Vec<ClipRep>)> {
        let count = clip.change_count();
        if count == self.seen {
            return None;
        }
        self.seen = count;
        let reps = clip.read().expect("clipboard readable");
        if reps.is_empty() || same_content(&reps, &self.last) {
            return None;
        }
        self.last = reps.clone();
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
