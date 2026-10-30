//! Saving a log bundle that the installed Mouser service streamed over the
//! control pipe (`logs.collect_stream`, owner decision OD-7): the service
//! writes no user path, so the user's own app reassembles the chunks, checks
//! them and writes the zip as the user.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use mouser_core::logs::bundle;
use mouser_proto::control::{LogsChunk, LogsCollectResult, LogsCollectStreamResult, RpcError};
use sha2::{Digest, Sha256};

/// The chunks joined in the order received.
fn reassemble(chunks: &[LogsChunk]) -> Vec<u8> {
    chunks
        .iter()
        .flat_map(|chunk| {
            base64::engine::general_purpose::STANDARD
                .decode(&chunk.data)
                .expect("a chunk is base64")
        })
        .collect()
}

/// Check the bundle against its `size` and `sha256`, then write it into
/// `dir` under `streamed.name` (` (2)`, ` (3)` when that name is taken).
/// A bundle that does not match leaves no file.
pub fn save(
    streamed: LogsCollectStreamResult,
    chunks: &[LogsChunk],
    dir: &Path,
) -> Result<LogsCollectResult, RpcError> {
    let zip = reassemble(chunks);
    let hash: String = Sha256::digest(&zip)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if zip.len() as u64 != streamed.size || hash != streamed.sha256 {
        return Err(RpcError {
            code: -32603,
            message: "The log bundle from the Mouser service arrived damaged, so it was not saved."
                .into(),
            data: None,
        });
    }
    let path: PathBuf = bundle::unique_path(dir, &streamed.name);
    std::fs::create_dir_all(dir).expect("the bundle folder can be created");
    std::fs::write(&path, &zip).expect("the bundle can be written");
    Ok(LogsCollectResult {
        path: path.to_string_lossy().into_owned(),
        computers: streamed.computers,
    })
}
