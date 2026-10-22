//! The transfer engine (Story 7): the state of every dragged-file transfer
//! this computer takes part in, as a pure state machine. [`Transfers::handle`]
//! takes what happened and returns what to do; the task in [`super`] does it.
//!
//! The source offers what is dragged. The destination asks the platform to
//! start a drag of the files and learns where they were dropped, checks the
//! space, accepts, and writes each file's stream under that folder, asking
//! about names that are taken.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ControlMessage, TransferId, TransferItem};
use serde_json::Value;

use super::drag::DRAG_DESTINATION;
use super::progress::{self, Progress};
use super::streams::{manifest, refuse_escapes};

/// Where a transfer is in its life (data-model "Transfer").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Pending,
    Checking,
    Copying,
    AwaitingOverwriteDecision,
    Completed,
    Rejected,
    /// The other computer went offline before the copy finished.
    Failed,
}

impl State {
    /// The name in `transfer` notifications.
    pub fn name(self) -> &'static str {
        match self {
            State::Pending => "pending",
            State::Checking => "checking",
            State::Copying => "copying",
            State::AwaitingOverwriteDecision => "awaiting_overwrite_decision",
            State::Completed => "completed",
            State::Rejected => "rejected",
            State::Failed => "failed",
        }
    }

    /// Whether the copy is still going on (or waiting to start).
    pub fn in_progress(self) -> bool {
        matches!(
            self,
            State::Pending | State::Checking | State::Copying | State::AwaitingOverwriteDecision
        )
    }
}

/// The transfers that belong to this engine, shared with the code that
/// routes messages and streams (clipboard files use the same wire messages).
#[derive(Clone, Default)]
pub struct Ids(Arc<Mutex<HashSet<TransferId>>>);

impl Ids {
    pub fn is_empty(&self) -> bool {
        self.0.lock().unwrap().is_empty()
    }

    pub fn contains(&self, id: &TransferId) -> bool {
        self.0.lock().unwrap().contains(id)
    }

    fn insert(&self, id: TransferId) {
        self.0.lock().unwrap().insert(id);
    }

    pub(super) fn remove(&self, id: &TransferId) {
        self.0.lock().unwrap().remove(id);
    }
}

/// Something for the transfer engine to react to.
#[derive(Debug)]
pub enum Command {
    /// A transfer message from a desk member.
    Message(ComputerId, ControlMessage),
    /// The pointer crossed to `to` while `paths` were being dragged.
    Drag { to: ComputerId, paths: Vec<PathBuf> },
    /// The dragged files were dropped in the folder `dir`.
    Dropped { id: TransferId, dir: PathBuf },
    /// The user cancelled the drag of the files of `id` (Esc, or a drop on
    /// something that is not a folder).
    Cancelled { id: TransferId },
    /// The stream of item `index` arrived.
    ItemStart { id: TransferId, index: usize },
    /// `bytes` more of the current item arrived.
    Chunk { id: TransferId, bytes: u64 },
    /// The stream of the current item ended.
    ItemDone { id: TransferId },
    /// The user answered the overwrite question of the transfer whose
    /// notification carried `transfer_id`.
    Resolve {
        transfer_id: String,
        overwrite: bool,
    },
    /// A file of the transfer being sent was received in full by the
    /// destination.
    Sent { id: TransferId },
    /// A desk member's connection ended; the name is how the user knows
    /// that computer.
    PeerGone(ComputerId, String),
}

/// What to do with a stream that has arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Write the file here, replacing what is there.
    Write(PathBuf),
    /// Read the stream and throw it away.
    Skip,
}

/// What the engine wants done.
#[derive(Debug, PartialEq)]
pub enum Out {
    To(ComputerId, ControlMessage),
    /// Stream the files of an accepted transfer to a desk member.
    Stream {
        to: ComputerId,
        transfer: TransferId,
        files: Vec<(TransferItem, PathBuf)>,
    },
    /// Ask the platform to start a drag of files with these names.
    Drag {
        id: TransferId,
        names: Vec<String>,
    },
    /// Tell the task reading the stream of item `index` what to do with it.
    Decide {
        id: TransferId,
        index: usize,
        decision: Decision,
    },
    /// Drop what is waiting to be told what to do with a stream of the
    /// transfer: its computer is gone.
    Forget {
        id: TransferId,
    },
    /// A `transfer` notification for the user interface.
    Notify(Value),
}

pub(super) struct Outbound {
    pub to: ComputerId,
    pub files: Vec<(TransferItem, PathBuf)>,
    pub state: State,
    /// Files the destination has received in full.
    pub sent: usize,
}

pub(super) struct Inbound {
    pub from: ComputerId,
    pub items: Vec<TransferItem>,
    /// Where the files were dropped, once they were.
    pub dir: PathBuf,
    pub state: State,
    pub progress: Progress,
    /// Files whose streams have not ended.
    pub remaining: usize,
    /// The item waiting for the user's answer. Items arrive one at a time.
    pub awaiting: Option<usize>,
    /// Files the user chose not to replace whose streams have ended.
    pub skipped: usize,
    /// The file now arriving is one the user chose not to replace.
    pub skipping: bool,
}

pub struct Transfers {
    /// Free bytes in the volume holding a folder.
    free: Box<dyn Fn(&Path) -> u64 + Send>,
    pub(super) ids: Ids,
    pub(super) outbound: HashMap<TransferId, Outbound>,
    pub(super) inbound: HashMap<TransferId, Inbound>,
    /// Transfers that ended because their computer went offline; what still
    /// arrives for them is of no interest.
    pub(super) failed: HashSet<TransferId>,
}

/// How many files (not folders) are among `items`.
pub(super) fn file_count(items: &[TransferItem]) -> usize {
    items.iter().filter(|item| !item.is_dir).count()
}

/// The bytes of the files among `items`.
pub(super) fn total_size(items: &[TransferItem]) -> u64 {
    items
        .iter()
        .filter(|item| !item.is_dir)
        .map(|item| item.size)
        .sum()
}

impl Transfers {
    pub fn new(ids: Ids, free: impl Fn(&Path) -> u64 + Send + 'static) -> Self {
        Self {
            free: Box::new(free),
            ids,
            outbound: HashMap::new(),
            inbound: HashMap::new(),
            failed: HashSet::new(),
        }
    }

    /// The state of transfer `id`, if it is known here.
    pub fn state_of(&self, id: &TransferId) -> Option<State> {
        let sending = self.outbound.get(id).map(|t| t.state);
        sending.or_else(|| self.inbound.get(id).map(|t| t.state))
    }

    pub fn handle(&mut self, command: Command, now: Instant) -> Result<Vec<Out>, String> {
        match command {
            Command::Message(from, message) => self.on_message(from, message, now),
            Command::Drag { to, paths } => Ok(self.offer(to, &paths)),
            Command::Dropped { id, dir } => self.on_dropped(id, dir, now),
            Command::Cancelled { id } => self.on_cancelled(id),
            Command::ItemStart { id, index } => self.on_item_start(id, index, now),
            Command::Chunk { id, bytes } => Ok(self.on_chunk(id, bytes, now)),
            Command::ItemDone { id } => Ok(self.on_item_done(id, now)),
            Command::Resolve {
                transfer_id,
                overwrite,
            } => self.resolve_named(&transfer_id, overwrite, now),
            Command::Sent { id } => {
                self.on_sent(id);
                Ok(Vec::new())
            }
            Command::PeerGone(peer, name) => Ok(self.peer_gone(peer, &name, now)),
        }
    }

    fn on_message(
        &mut self,
        from: ComputerId,
        message: ControlMessage,
        now: Instant,
    ) -> Result<Vec<Out>, String> {
        match message {
            ControlMessage::TransferOffer {
                transfer_id, items, ..
            } => self.on_offer(from, transfer_id, items),
            ControlMessage::TransferAccept { transfer_id } => self.on_accept(from, transfer_id),
            ControlMessage::TransferReject {
                transfer_id,
                reason,
            } => self.on_reject(from, transfer_id, &reason, now),
            // The destination's question; its user answers it there.
            ControlMessage::TransferConflict { .. } => Ok(Vec::new()),
            ControlMessage::TransferResolve {
                transfer_id,
                overwrite,
            } => {
                let sent_by_source = self
                    .inbound
                    .get(&transfer_id)
                    .is_some_and(|t| t.from == from);
                if !sent_by_source {
                    return Err("TransferResolve for a transfer that is not ours to answer".into());
                }
                self.resolve(transfer_id, overwrite, now)
            }
            ControlMessage::TransferDone { transfer_id } => self.on_done(from, transfer_id),
            other => Err(format!("unexpected message {other:?}")),
        }
    }

    /// The source: the pointer crossed to `to` with `paths` in the drag.
    fn offer(&mut self, to: ComputerId, paths: &[PathBuf]) -> Vec<Out> {
        let files = manifest(paths);
        let items = files.iter().map(|(item, _)| item.clone()).collect();
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).expect("operating system randomness");
        let id = TransferId(id);
        self.ids.insert(id);
        self.outbound.insert(
            id,
            Outbound {
                to,
                files,
                state: State::Pending,
                sent: 0,
            },
        );
        vec![Out::To(
            to,
            ControlMessage::TransferOffer {
                transfer_id: id,
                items,
                // Only the destination knows where the files will be dropped.
                destination_dir: DRAG_DESTINATION.into(),
            },
        )]
    }

    /// The destination: files are being dragged to us.
    fn on_offer(
        &mut self,
        from: ComputerId,
        id: TransferId,
        items: Vec<TransferItem>,
    ) -> Result<Vec<Out>, String> {
        refuse_escapes(&items)?;
        let names = items
            .iter()
            .filter(|item| !item.relative_path.contains('/'))
            .map(|item| item.relative_path.clone())
            .collect();
        self.ids.insert(id);
        self.inbound.insert(
            id,
            Inbound {
                from,
                dir: PathBuf::new(),
                state: State::Pending,
                progress: Progress::new(total_size(&items)),
                remaining: file_count(&items),
                awaiting: None,
                skipped: 0,
                skipping: false,
                items,
            },
        );
        Ok(vec![Out::Drag { id, names }])
    }

    /// The source: the destination will take the files.
    fn on_accept(&mut self, from: ComputerId, id: TransferId) -> Result<Vec<Out>, String> {
        let offered = self.outbound.get_mut(&id).filter(|t| t.to == from);
        let Some(transfer) = offered else {
            return Err("TransferAccept for a transfer we did not offer".into());
        };
        transfer.state = State::Copying;
        Ok(vec![Out::Stream {
            to: from,
            transfer: id,
            files: transfer.files.clone(),
        }])
    }

    /// The source: the destination will not take the files.
    fn on_reject(
        &mut self,
        from: ComputerId,
        id: TransferId,
        reason: &str,
        now: Instant,
    ) -> Result<Vec<Out>, String> {
        if self.outbound.get(&id).is_none_or(|t| t.to != from) {
            return Err("TransferReject for a transfer we did not offer".into());
        }
        let rejected = self.outbound.remove(&id).expect("checked above");
        self.ids.remove(&id);
        let items: Vec<_> = rejected
            .files
            .iter()
            .map(|(item, _)| item.clone())
            .collect();
        let snapshot = Progress::new(total_size(&items)).snapshot(now);
        let mut note = progress::notification(&id, State::Rejected, &snapshot);
        note["reason"] = reason.into();
        note["message"] = super::space::NO_SPACE_TEXT.into();
        Ok(vec![Out::Notify(note)])
    }

    /// The source: the destination has everything.
    fn on_done(&mut self, from: ComputerId, id: TransferId) -> Result<Vec<Out>, String> {
        if self.outbound.get(&id).is_none_or(|t| t.to != from) {
            return Err("TransferDone for a transfer we did not offer".into());
        }
        self.outbound.remove(&id);
        self.ids.remove(&id);
        Ok(Vec::new())
    }

    /// The destination: the files were dropped in `dir`.
    fn on_dropped(
        &mut self,
        id: TransferId,
        dir: PathBuf,
        now: Instant,
    ) -> Result<Vec<Out>, String> {
        if self.failed.contains(&id) {
            return Ok(Vec::new());
        }
        let free = (self.free)(&dir);
        let Some(transfer) = self.inbound.get_mut(&id) else {
            return Err("a drop for files nobody offered".into());
        };
        transfer.dir = dir;
        transfer.state = State::Checking;
        if !super::space::fits(total_size(&transfer.items), free) {
            return Ok(self.reject_for_space(id, now));
        }
        for item in transfer.items.iter().filter(|item| item.is_dir) {
            std::fs::create_dir_all(transfer.dir.join(&item.relative_path))
                .expect("dropped folder");
        }
        transfer.state = State::Copying;
        let mut outs = vec![
            Out::To(
                transfer.from,
                ControlMessage::TransferAccept { transfer_id: id },
            ),
            Out::Notify(progress::notification(
                &id,
                State::Copying,
                &transfer.progress.snapshot(now),
            )),
        ];
        if transfer.remaining == 0 {
            outs.extend(self.finish(id, now));
        }
        Ok(outs)
    }

    /// The destination: the drag ended without a drop. Nothing was written
    /// or shown; the source is told it is done, as if the files had been
    /// dropped back where they started.
    fn on_cancelled(&mut self, id: TransferId) -> Result<Vec<Out>, String> {
        if self.failed.contains(&id) {
            return Ok(Vec::new());
        }
        let Some(transfer) = self.inbound.remove(&id) else {
            return Err("a cancel for files nobody offered".into());
        };
        self.ids.remove(&id);
        Ok(vec![Out::To(
            transfer.from,
            ControlMessage::TransferDone { transfer_id: id },
        )])
    }

    /// The destination: bytes of the current file arrived.
    fn on_chunk(&mut self, id: TransferId, bytes: u64, now: Instant) -> Vec<Out> {
        // The transfer ends when its computer goes; bytes already on the way
        // are of no interest then.
        let Some(transfer) = self.inbound.get_mut(&id) else {
            return Vec::new();
        };
        transfer.progress.add(bytes, now);
        if !transfer.progress.due(now) {
            return Vec::new();
        }
        vec![self.report(id, now)]
    }

    /// The destination: a file's stream ended.
    fn on_item_done(&mut self, id: TransferId, now: Instant) -> Vec<Out> {
        let Some(transfer) = self.inbound.get_mut(&id) else {
            return Vec::new();
        };
        transfer.remaining -= 1;
        if std::mem::take(&mut transfer.skipping) {
            transfer.skipped += 1;
        }
        if transfer.remaining > 0 {
            return vec![self.report(id, now)];
        }
        self.finish(id, now)
    }

    /// The destination: everything is here.
    fn finish(&mut self, id: TransferId, now: Instant) -> Vec<Out> {
        let transfer = self
            .inbound
            .get_mut(&id)
            .expect("a finished transfer exists");
        transfer.state = State::Completed;
        let from = transfer.from;
        self.ids.remove(&id);
        vec![
            Out::To(from, ControlMessage::TransferDone { transfer_id: id }),
            self.report(id, now),
        ]
    }

    /// The notification for the transfer as it is.
    pub(super) fn report(&self, id: TransferId, now: Instant) -> Out {
        let transfer = &self.inbound[&id];
        Out::Notify(progress::notification(
            &id,
            transfer.state,
            &transfer.progress.snapshot(now),
        ))
    }
}
