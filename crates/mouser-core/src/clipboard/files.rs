//! Copied files and folders (Story 6 scenario 2).
//!
//! A copy sends only a manifest (names, sizes, relative paths). Pasting asks
//! the origin for the files; it answers with a transfer: `TransferOffer`,
//! our `TransferAccept`, then one unidirectional stream per file, each
//! starting with the transfer id and the item's index in the offer. The files
//! are written under a directory of their own and the clipboard then holds
//! those local copies.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

use mouser_platform::{ClipRep, Clipboard};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ClipboardKind, ControlMessage, TransferId, TransferItem};
use tokio::io::AsyncWriteExt;

use super::{Clip, Out};

/// Every file or folder under `paths`, with its path relative to the folder
/// the copied item sits in (so a copied folder keeps its name).
pub fn manifest(paths: &[PathBuf]) -> Vec<(TransferItem, PathBuf)> {
    let mut items = Vec::new();
    for path in paths {
        let name = path.file_name().expect("a copied path has a name");
        walk(path, name.to_str().expect("UTF-8 name"), &mut items);
    }
    items
}

fn walk(path: &Path, relative: &str, items: &mut Vec<(TransferItem, PathBuf)>) {
    let meta = std::fs::metadata(path).expect("copied file readable");
    let is_dir = meta.is_dir();
    items.push((
        TransferItem {
            relative_path: relative.to_string(),
            size: if is_dir { 0 } else { meta.len() },
            is_dir,
        },
        path.to_path_buf(),
    ));
    if is_dir {
        let mut names: Vec<_> = std::fs::read_dir(path)
            .expect("copied folder readable")
            .map(|entry| entry.expect("folder entry readable").file_name())
            .collect();
        names.sort();
        for name in names {
            let name = name.to_str().expect("UTF-8 name");
            walk(&path.join(name), &format!("{relative}/{name}"), items);
        }
    }
}

/// The transfers we expect streams for.
#[derive(Clone, Default)]
pub struct Inbox(Arc<Mutex<HashMap<TransferId, Incoming>>>);

struct Incoming {
    dir: PathBuf,
    items: Vec<TransferItem>,
    /// Files whose streams have not finished.
    remaining: usize,
}

/// The items directly in the transfer's directory: what the clipboard holds.
fn top_level(dir: &Path, items: &[TransferItem]) -> Vec<PathBuf> {
    items
        .iter()
        .filter(|item| !item.relative_path.contains('/'))
        .map(|item| dir.join(&item.relative_path))
        .collect()
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
        for item in items {
            let escapes = Path::new(&item.relative_path)
                .components()
                .any(|part| !matches!(part, Component::Normal(_)));
            if escapes {
                return Err(format!("{:?} is outside the transfer", item.relative_path));
            }
        }
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

    /// Read one file's stream; the top-level paths once the last file is in.
    pub async fn receive(
        &self,
        mut stream: quinn::RecvStream,
    ) -> std::io::Result<Option<Vec<PathBuf>>> {
        let mut header = [0u8; 20];
        stream
            .read_exact(&mut header)
            .await
            .map_err(std::io::Error::other)?;
        let id = TransferId(header[..16].try_into().expect("16 bytes"));
        let index = u32::from_be_bytes(header[16..].try_into().expect("4 bytes")) as usize;
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

/// Send the files of a transfer, one unidirectional stream each.
pub async fn send(
    conn: &quinn::Connection,
    id: TransferId,
    files: &[(TransferItem, PathBuf)],
) -> std::io::Result<()> {
    for (index, (item, path)) in files.iter().enumerate() {
        if item.is_dir {
            continue;
        }
        let mut stream = conn.open_uni().await.map_err(std::io::Error::other)?;
        let mut header = id.0.to_vec();
        header.extend((index as u32).to_be_bytes());
        stream.write_all(&header).await?;
        let mut file = tokio::fs::File::open(path).await?;
        tokio::io::copy(&mut file, &mut stream).await?;
        stream.finish().map_err(std::io::Error::other)?;
    }
    Ok(())
}

fn hex(id: &TransferId) -> String {
    id.0.iter().map(|byte| format!("{byte:02x}")).collect()
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
