//! Copied files and folders (Story 6 scenario 2).
//!
//! A copy sends only a manifest (names, sizes, relative paths). Pasting asks
//! the origin for the files; it answers with a transfer: `TransferOffer`,
//! our `TransferAccept`, then one unidirectional stream per file, each
//! starting with the transfer id and the item's index in the offer. The files
//! are written under a directory of their own and the clipboard then holds
//! those local copies.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mouser_platform::{ClipRep, Clipboard};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ClipboardKind, ControlMessage, TransferId, TransferItem};
use tokio::io::AsyncWriteExt;

use super::{Clip, Out};
use crate::transfer::streams::{hex, refuse_escapes, top_level};

/// The transfers we expect streams for.
#[derive(Clone, Default)]
pub struct Inbox(Arc<Mutex<HashMap<TransferId, Incoming>>>);

struct Incoming {
    dir: PathBuf,
    items: Vec<TransferItem>,
    /// Files whose streams have not finished.
    remaining: usize,
}

impl Inbox {
    /// Expect `items` under `dir`: make the folders, and refuse any path that
    /// would leave `dir`. The top-level paths if there are no files to wait for.
    pub fn expect(
        &self,
        id: TransferId,
        dir: &Path,
        items: &[TransferItem],
    ) -> Result<Option<Vec<PathBuf>>, String> {
        refuse_escapes(items)?;
        std::fs::create_dir_all(dir).expect("transfer directory");
        for item in items.iter().filter(|item| item.is_dir) {
            std::fs::create_dir_all(dir.join(&item.relative_path)).expect("transfer folder");
        }
        let remaining = items.iter().filter(|item| !item.is_dir).count();
        if remaining == 0 {
            return Ok(Some(top_level(dir, items)));
        }
        self.0.lock().unwrap().insert(
            id,
            Incoming {
                dir: dir.to_path_buf(),
                items: items.to_vec(),
                remaining,
            },
        );
        Ok(None)
    }

    /// Read file `index` of transfer `id` from its stream; the top-level
    /// paths once the last file is in.
    pub async fn receive(
        &self,
        id: TransferId,
        index: usize,
        mut stream: quinn::RecvStream,
    ) -> std::io::Result<Option<Vec<PathBuf>>> {
        let path = {
            let transfers = self.0.lock().unwrap();
            let incoming = transfers.get(&id).expect("a transfer we expect");
            incoming.dir.join(&incoming.items[index].relative_path)
        };
        let mut file = tokio::fs::File::create(&path).await?;
        tokio::io::copy(&mut stream, &mut file).await?;
        file.flush().await?;
        let mut transfers = self.0.lock().unwrap();
        let incoming = transfers.get_mut(&id).expect("a transfer we expect");
        incoming.remaining -= 1;
        if incoming.remaining > 0 {
            return Ok(None);
        }
        let done = transfers.remove(&id).expect("just found");
        Ok(Some(top_level(&done.dir, &done.items)))
    }
}

impl Clip {
    /// The origin: a desk member wants the files we copied.
    pub(super) fn offer_files(&mut self, to: ComputerId) -> Vec<Out> {
        let files = self
            .served
            .as_ref()
            .expect("serving an offer")
            .files
            .clone();
        let mut id = [0u8; 16];
        getrandom::fill(&mut id).expect("operating system randomness");
        let id = TransferId(id);
        let items = files.iter().map(|(item, _)| item.clone()).collect();
        self.outbound.insert(id, (to, files));
        vec![Out::To(
            to,
            ControlMessage::TransferOffer {
                transfer_id: id,
                items,
                // The receiver decides where pasted files go.
                destination_dir: String::new(),
            },
        )]
    }

    /// The paster: the origin offers the files we asked for.
    pub fn on_transfer_offer(
        &mut self,
        clip: &mut dyn Clipboard,
        from: ComputerId,
        id: TransferId,
        items: &[TransferItem],
    ) -> Result<Vec<Out>, String> {
        let asked = self.fetching.as_mut().filter(|fetching| {
            fetching.origin == from && fetching.waiting.contains(&ClipboardKind::Files)
        });
        let Some(fetching) = asked else {
            return Err("TransferOffer for files nobody asked for".into());
        };
        fetching.transfer = Some(id);
        let dir = self.dir.join(hex(&id));
        let done = self.inbox.expect(id, &dir, items)?;
        let mut outs = vec![Out::To(
            from,
            ControlMessage::TransferAccept { transfer_id: id },
        )];
        if let Some(paths) = done {
            outs.extend(self.files_arrived(clip, paths));
        }
        Ok(outs)
    }

    /// The origin: the paster accepted, so the streams may start.
    pub fn on_transfer_accept(
        &mut self,
        from: ComputerId,
        id: TransferId,
    ) -> Result<Vec<Out>, String> {
        if self.outbound.get(&id).is_none_or(|(to, _)| *to != from) {
            return Err("TransferAccept for a transfer we did not offer".into());
        }
        let (to, files) = self.outbound.remove(&id).expect("checked above");
        Ok(vec![Out::Stream {
            to,
            transfer: id,
            files,
        }])
    }

    /// All of the files are here: they are part of what is being fetched.
    pub(super) fn files_arrived(
        &mut self,
        clip: &mut dyn Clipboard,
        paths: Vec<PathBuf>,
    ) -> Vec<Out> {
        let Some(fetching) = &mut self.fetching else {
            return Vec::new();
        };
        fetching.add(ClipboardKind::Files, ClipRep::Files(paths));
        let done = fetching.transfer.take().map(|id| {
            Out::To(
                fetching.origin,
                ControlMessage::TransferDone { transfer_id: id },
            )
        });
        self.finish_if_complete(clip);
        done.into_iter().collect()
    }
}
