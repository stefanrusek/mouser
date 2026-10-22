//! What clipboard files and dragged files share: the manifest of a set of
//! paths, the refusal of paths that leave their directory, and the
//! per-file unidirectional streams (a 16-byte transfer id and a 4-byte item
//! index, then the file's bytes).

use std::path::{Component, Path, PathBuf};

use mouser_proto::wire::{TransferId, TransferItem};

/// Every file or folder under `paths`, with its path relative to the folder
/// the item sits in (so a folder keeps its name).
pub fn manifest(paths: &[PathBuf]) -> Vec<(TransferItem, PathBuf)> {
    let mut items = Vec::new();
    for path in paths {
        let name = path.file_name().expect("a path to transfer has a name");
        walk(path, name.to_str().expect("UTF-8 name"), &mut items);
    }
    items
}

fn walk(path: &Path, relative: &str, items: &mut Vec<(TransferItem, PathBuf)>) {
    let meta = std::fs::metadata(path).expect("file to transfer readable");
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
            .expect("folder to transfer readable")
            .map(|entry| entry.expect("folder entry readable").file_name())
            .collect();
        names.sort();
        for name in names {
            let name = name.to_str().expect("UTF-8 name");
            walk(&path.join(name), &format!("{relative}/{name}"), items);
        }
    }
}

/// Refuse any item whose path would leave the directory it is written under.
pub fn refuse_escapes(items: &[TransferItem]) -> Result<(), String> {
    for item in items {
        let escapes = Path::new(&item.relative_path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)));
        if escapes {
            return Err(format!("{:?} is outside the transfer", item.relative_path));
        }
    }
    Ok(())
}

/// The items directly in the transfer's directory.
pub fn top_level(dir: &Path, items: &[TransferItem]) -> Vec<PathBuf> {
    items
        .iter()
        .filter(|item| !item.relative_path.contains('/'))
        .map(|item| dir.join(&item.relative_path))
        .collect()
}

/// The transfer id as the lowercase hex the user interface is given.
pub fn hex(id: &TransferId) -> String {
    id.0.iter().map(|byte| format!("{byte:02x}")).collect()
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

/// Which file of which transfer an incoming stream carries.
pub async fn read_header(stream: &mut quinn::RecvStream) -> std::io::Result<(TransferId, usize)> {
    let mut header = [0u8; 20];
    stream
        .read_exact(&mut header)
        .await
        .map_err(std::io::Error::other)?;
    let id = TransferId(header[..16].try_into().expect("16 bytes"));
    let index = u32::from_be_bytes(header[16..].try_into().expect("4 bytes")) as usize;
    Ok((id, index))
}
