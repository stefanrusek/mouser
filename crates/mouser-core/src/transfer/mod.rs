//! Dragging files from one computer to another (Story 7).
//!
//! One task owns the [`engine::Transfers`] state machine. What happens to
//! it arrives as a [`engine::Command`]: the pointer crossing with files
//! in the drag, desk messages, the file streams, the user's answers. What it
//! decides comes back as [`engine::Out`]s, which the task puts on the wire,
//! hands to the platform or tells the user interface.
//!
//! [`streams`] is shared with clipboard files, which use the same messages
//! and the same per-file streams.

pub mod conflict;
pub mod drag;
pub mod engine;
pub mod interrupt;
pub mod progress;
pub mod space;
pub mod streams;

#[cfg(test)]
mod conflict_tests;
#[cfg(test)]
mod drag_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod interrupt_tests;
#[cfg(test)]
mod progress_tests;
#[cfg(test)]
mod space_tests;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use mouser_platform::FileDrag;
use mouser_proto::control::{Notification, Topic};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ControlMessage, TransferId, TransferItem};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;

use crate::engine::Ctx;
use engine::{Command, Decision, Out, Transfers};

/// Streams waiting to be told what to do with their file.
pub type Waiters = Arc<Mutex<HashMap<(TransferId, usize), oneshot::Sender<Decision>>>>;

/// How much of a file is read before progress is told.
const CHUNK: usize = 64 * 1024;

impl Ctx {
    /// A transfer message from `peer`: dragged files or pasted files.
    pub(crate) fn transfer_message(
        &self,
        peer: ComputerId,
        message: ControlMessage,
    ) -> Result<(), String> {
        if !drag::is_drag(&message, &self.transfer_ids) {
            return self.clipboard_message(peer, message);
        }
        if !Self::authed(&self.core.lock().unwrap(), peer) {
            return Err("file transfer traffic from a computer that is not on the desk".into());
        }
        // No transfer task means this platform cannot drag files.
        let _ = self.xfer.send(Command::Message(peer, message));
        Ok(())
    }

    /// Stream the files of an accepted transfer; a lost connection ends the
    /// transfer, which the other end notices.
    pub(crate) fn stream_files(
        &self,
        conn: quinn::Connection,
        transfer: TransferId,
        files: Vec<(TransferItem, std::path::PathBuf)>,
    ) {
        let commands = self.xfer.clone();
        self.spawn(async move {
            let sent = || {
                // No transfer task means the files are pasted, not dragged.
                let _ = commands.send(Command::Sent { id: transfer });
            };
            let _ = streams::send(&conn, transfer, &files, sent).await;
        });
    }

    /// Run the transfer task if the platform can drag files.
    pub(crate) fn start_transfers(self: &Arc<Self>, commands: UnboundedReceiver<Command>) {
        let Some(drag) = self.platform.file_drag() else {
            return;
        };
        let transfers = Transfers::new(self.transfer_ids.clone(), space::free_space);
        self.spawn(transfer_task(
            self.clone(),
            Arc::new(Mutex::new(drag)),
            transfers,
            commands,
        ));
    }

    /// Put what the engine decided into effect.
    fn emit_transfer(&self, drag: &Arc<Mutex<Box<dyn FileDrag>>>, outs: Vec<Out>) {
        for out in outs {
            match out {
                Out::To(peer, message) => {
                    if let Some(entry) = self.core.lock().unwrap().peers.get(&peer) {
                        entry.send(message);
                    }
                }
                Out::Stream {
                    to,
                    transfer,
                    files,
                } => {
                    if let Some(entry) = self.core.lock().unwrap().peers.get(&to) {
                        self.stream_files(entry.conn.clone(), transfer, files);
                    }
                }
                Out::Drag { id, names } => {
                    let (drag, commands) = (drag.clone(), self.xfer.clone());
                    // The drag runs until the user drops; the platform decides how long.
                    std::thread::Builder::new()
                        .name("mouser-drag".into())
                        .spawn(move || {
                            let dropped = drag.lock().unwrap().begin_incoming(names);
                            // A drag that was cancelled ends quietly. The engine
                            // outlives the drag unless it is shutting down.
                            let _ = commands.send(match dropped {
                                Ok(target) => Command::Dropped { id, dir: target.0 },
                                Err(_) => Command::Cancelled { id },
                            });
                        })
                        .expect("drag thread starts");
                }
                Out::Decide {
                    id,
                    index,
                    decision,
                } => {
                    let waiting = self.xfer_waiters.lock().unwrap().remove(&(id, index));
                    // The stream may have been cut while the user decided.
                    let _ = waiting.expect("a stream waits for this").send(decision);
                }
                Out::Forget { id } => {
                    self.xfer_waiters
                        .lock()
                        .unwrap()
                        .retain(|(waiting, _), _| *waiting != id);
                }
                Out::Notify(params) => {
                    // No subscribers is fine.
                    let _ = self.notify.send(Notification {
                        jsonrpc: "2.0".into(),
                        method: Topic::Transfer,
                        params,
                    });
                }
            }
        }
    }

    /// One dragged file's stream: written where the engine says, with
    /// progress; or thrown away.
    pub(crate) async fn receive_dragged(
        self: Arc<Self>,
        id: TransferId,
        index: usize,
        mut stream: quinn::RecvStream,
    ) {
        let (tell, told) = oneshot::channel();
        self.xfer_waiters.lock().unwrap().insert((id, index), tell);
        // The engine runs whenever a transfer id is known.
        let _ = self.xfer.send(Command::ItemStart { id, index });
        // The engine gives up on the transfer when its computer goes offline.
        let Ok(decision) = told.await else {
            return;
        };
        let commands = self.xfer.clone();
        let copied = async {
            match &decision {
                Decision::Write(path) => {
                    let mut file = tokio::fs::File::create(path).await?;
                    let mut buffer = vec![0u8; CHUNK];
                    loop {
                        let Some(read) = stream
                            .read(&mut buffer)
                            .await
                            .map_err(std::io::Error::other)?
                        else {
                            break;
                        };
                        file.write_all(&buffer[..read]).await?;
                        let _ = commands.send(Command::Chunk {
                            id,
                            bytes: read as u64,
                        });
                    }
                    file.flush().await
                }
                Decision::Skip => tokio::io::copy(&mut stream, &mut tokio::io::sink())
                    .await
                    .map(|_| ()),
            }
        };
        // A stream cut short by a lost connection is a transfer that did not
        // finish: what was written of the file is not kept.
        if copied.await.is_ok() {
            let _ = self.xfer.send(Command::ItemDone { id });
        } else if let Decision::Write(path) = &decision {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
}

async fn transfer_task(
    ctx: Arc<Ctx>,
    drag: Arc<Mutex<Box<dyn FileDrag>>>,
    mut transfers: Transfers,
    mut commands: UnboundedReceiver<Command>,
) {
    while let Some(command) = commands.recv().await {
        let outs = transfers
            .handle(command, Instant::now())
            .expect("peer followed the protocol");
        ctx.emit_transfer(&drag, outs);
    }
}
