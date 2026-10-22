//! Clipboard sharing (Story 6): what is copied on one computer can be pasted
//! on the others.
//!
//! One task owns the platform clipboard and a [`Clip`]. Everything that
//! touches it arrives as a [`Command`]: desk messages, the paste shortcut and
//! the targeted-paste hotkey (both seen by the input path), and files that
//! finished arriving. What the clip decides to send comes back as [`Out`]s,
//! which the task puts on the wire.

pub mod apply;
pub mod files;
pub mod targeted;
pub mod watch;

#[cfg(test)]
mod apply_tests;
#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod targeted_tests;
#[cfg(test)]
mod watch_tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use mouser_platform::{ClipRep, Clipboard};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ClipboardKind, ControlMessage, TransferId, TransferItem};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::engine::{Ctx, Phase};
use apply::{Fetching, Remote};
use files::Inbox;
use watch::{Served, Watcher};

/// Representations up to this size travel inside the offer.
pub const INLINE_LIMIT: usize = 1 << 20;

/// How often the clipboard's change counter is looked at.
pub const POLL: Duration = Duration::from_millis(100);

/// The wire kind of a representation.
pub fn kind_of(rep: &ClipRep) -> ClipboardKind {
    match rep {
        ClipRep::Text(_) => ClipboardKind::TextPlain,
        ClipRep::Html(_) => ClipboardKind::TextHtml,
        ClipRep::Rtf(_) => ClipboardKind::TextRtf,
        ClipRep::Png(_) => ClipboardKind::ImagePng,
        ClipRep::Files(_) => ClipboardKind::Files,
    }
}

/// The bytes of a representation; files have none (they travel as transfers).
pub fn encode(rep: &ClipRep) -> Option<Vec<u8>> {
    match rep {
        ClipRep::Text(text) => Some(text.clone().into_bytes()),
        ClipRep::Html(html) => Some(html.clone().into_bytes()),
        ClipRep::Rtf(bytes) | ClipRep::Png(bytes) => Some(bytes.clone()),
        ClipRep::Files(_) => None,
    }
}

/// The representation of `kind` held in `bytes`.
pub fn decode(kind: ClipboardKind, bytes: Vec<u8>) -> ClipRep {
    let text = |bytes| String::from_utf8(bytes).expect("clipboard text is UTF-8");
    match kind {
        ClipboardKind::TextPlain => ClipRep::Text(text(bytes)),
        ClipboardKind::TextHtml => ClipRep::Html(text(bytes)),
        ClipboardKind::TextRtf => ClipRep::Rtf(bytes),
        ClipboardKind::ImagePng => ClipRep::Png(bytes),
        ClipboardKind::Files => panic!("files are never carried inline"),
    }
}

/// Something for the clipboard task to do.
#[derive(Debug)]
pub enum Command {
    /// A clipboard or transfer message from a desk member.
    Message(ComputerId, ControlMessage),
    /// The paste shortcut was pressed on this computer.
    Paste,
    /// The targeted-paste hotkey was pressed on this computer.
    Targeted,
    /// A desk member's connection ended.
    PeerGone(ComputerId),
    /// This computer paused (`true`) or resumed.
    Paused(bool),
    /// A paused desk member resumed.
    PeerResumed(ComputerId),
    /// The files of a transfer are all here (their top-level paths).
    FilesArrived(Vec<PathBuf>),
}

/// What the clip wants done.
#[derive(Debug, PartialEq)]
pub enum Out {
    To(ComputerId, ControlMessage),
    /// To every desk member.
    All(ControlMessage),
    /// Stream the files of an accepted transfer to a desk member.
    Stream {
        to: ComputerId,
        transfer: TransferId,
        files: Vec<(TransferItem, PathBuf)>,
    },
}

/// Clipboard sharing state of one computer.
pub struct Clip {
    pub(crate) watcher: Watcher,
    /// What we offered last, to serve fetches.
    pub(crate) served: Option<Served>,
    /// Files we offered to send, by transfer, and who asked.
    pub(crate) outbound: HashMap<TransferId, (ComputerId, Vec<(TransferItem, PathBuf)>)>,
    /// The latest offer from another computer.
    pub(crate) remote: Option<Remote>,
    pub(crate) fetching: Option<Fetching>,
    /// Where pasted files are written.
    pub(crate) dir: PathBuf,
    pub(crate) inbox: Inbox,
    /// This computer is paused: it shares nothing, though it notices copies.
    pub(crate) paused: bool,
    /// The clipboard holds what the user copied here (`served`), not
    /// something that came from another computer.
    pub(crate) mine: bool,
}

impl Clip {
    /// Start sharing; what the clipboard holds now is not shared.
    pub fn new(clip: &mut dyn Clipboard, dir: PathBuf, inbox: Inbox) -> Self {
        Self {
            watcher: Watcher::new(clip),
            served: None,
            outbound: HashMap::new(),
            remote: None,
            fetching: None,
            dir,
            inbox,
            paused: false,
            mine: false,
        }
    }

    pub fn handle(
        &mut self,
        clip: &mut dyn Clipboard,
        command: Command,
    ) -> Result<Vec<Out>, String> {
        match command {
            Command::Message(from, message) => self.on_message(clip, from, message),
            Command::Paste => Ok(self.paste()),
            Command::Targeted => Ok(self.targeted()),
            Command::PeerGone(peer) => {
                self.peer_gone(peer);
                Ok(Vec::new())
            }
            Command::FilesArrived(paths) => Ok(self.files_arrived(clip, paths)),
            Command::Paused(paused) => {
                self.paused = paused;
                Ok(self.offer_mine(None))
            }
            Command::PeerResumed(peer) => Ok(self.offer_mine(Some(peer))),
        }
    }

    fn on_message(
        &mut self,
        clip: &mut dyn Clipboard,
        from: ComputerId,
        message: ControlMessage,
    ) -> Result<Vec<Out>, String> {
        match message {
            ControlMessage::ClipboardOffer {
                sequence,
                representations,
                ..
            } => {
                self.on_offer(clip, from, sequence, representations);
                Ok(Vec::new())
            }
            ControlMessage::ClipboardFetch { sequence, kind } => {
                Ok(self.on_fetch(from, sequence, kind))
            }
            ControlMessage::ClipboardData {
                sequence,
                kind,
                bytes,
            } => {
                self.on_data(clip, from, sequence, kind, bytes);
                Ok(Vec::new())
            }
            ControlMessage::TransferOffer {
                transfer_id, items, ..
            } => self.on_transfer_offer(clip, from, transfer_id, &items),
            ControlMessage::TransferAccept { transfer_id } => {
                self.on_transfer_accept(from, transfer_id)
            }
            // The paster's courtesy; the files were handed over at the accept.
            ControlMessage::TransferDone { .. } => Ok(Vec::new()),
            other => Err(format!("unexpected message {other:?}")),
        }
    }
}

impl Ctx {
    /// A clipboard or transfer message from `peer`.
    pub(crate) fn clipboard_message(
        &self,
        peer: ComputerId,
        message: ControlMessage,
    ) -> Result<(), String> {
        let core = self.core.lock().unwrap();
        if !Self::authed(&core, peer) {
            return Err("clipboard traffic from a computer that is not on the desk".into());
        }
        // Nothing is shared with or by a paused computer.
        if self.sharing_off(&core, peer) {
            return Ok(());
        }
        // No clipboard task means this platform shares no clipboard.
        let _ = self.clip.send(Command::Message(peer, message));
        Ok(())
    }

    /// Put what the clip decided on the wire.
    fn emit(&self, outs: Vec<Out>) {
        let core = self.core.lock().unwrap();
        for out in outs {
            match out {
                Out::To(peer, message) => {
                    if let Some(entry) = core
                        .peers
                        .get(&peer)
                        .filter(|_| !self.sharing_off(&core, peer))
                    {
                        entry.send(message);
                    }
                }
                Out::All(message) => {
                    for entry in core
                        .peers
                        .values()
                        .filter(|p| p.phase == Phase::Authed && !p.paused && !core.paused)
                    {
                        entry.send(message.clone());
                    }
                }
                Out::Stream {
                    to,
                    transfer,
                    files,
                } => {
                    if let Some(entry) =
                        core.peers.get(&to).filter(|_| !self.sharing_off(&core, to))
                    {
                        self.stream_files(entry.conn.clone(), transfer, files);
                    }
                }
            }
        }
    }

    /// Run the clipboard task if the platform has a clipboard.
    pub(crate) fn start_clipboard(self: &Arc<Self>, commands: UnboundedReceiver<Command>) {
        let Some(mut clipboard) = crate::crossing::supported(self.platform.clipboard()) else {
            return;
        };
        let clip = Clip::new(
            &mut *clipboard,
            self.config_dir.join("clipboard"),
            self.clip_inbox.clone(),
        );
        self.spawn(clipboard_task(self.clone(), clipboard, clip, commands));
    }
}

async fn clipboard_task(
    ctx: Arc<Ctx>,
    mut clipboard: Box<dyn Clipboard>,
    mut clip: Clip,
    mut commands: UnboundedReceiver<Command>,
) {
    let mut tick = tokio::time::interval(POLL);
    loop {
        let outs = tokio::select! {
            _ = tick.tick() => clip.poll(&mut *clipboard),
            Some(command) = commands.recv() => clip
                .handle(&mut *clipboard, command)
                .expect("peer followed the protocol"),
        };
        ctx.emit(outs);
    }
}
