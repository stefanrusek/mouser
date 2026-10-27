//! `logs.collect_stream` (owner decision OD-7): the bundle of `logs.collect`,
//! built in memory and sent to the calling connection as `logs.chunk`
//! notifications; the caller writes the file, as the user.

use std::sync::Arc;

use base64::Engine as _;
use mouser_proto::control::{LogsChunk, LogsCollectStreamResult, RpcError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc::UnboundedSender;

use crate::engine::Ctx;
use crate::logs::bundle;

/// The most bytes in one chunk, before base64.
const CHUNK: usize = 65536;

impl Ctx {
    /// Build the bundle and send its bytes down `chunks`, in order, then
    /// describe it.
    pub(crate) async fn logs_collect_stream(
        self: &Arc<Self>,
        chunks: &UnboundedSender<Value>,
    ) -> Result<LogsCollectStreamResult, RpcError> {
        let built = self.build_bundle().await;
        for (seq, bytes) in built.zip.chunks(CHUNK).enumerate() {
            let chunk = LogsChunk {
                seq: seq as u32,
                data: base64::engine::general_purpose::STANDARD.encode(bytes),
            };
            // A caller that is gone has no use for the rest.
            let _ = chunks.send(json!({
                "jsonrpc": "2.0",
                "method": LogsChunk::METHOD,
                "params": chunk,
            }));
        }
        Ok(LogsCollectStreamResult {
            name: built.name,
            size: built.zip.len() as u64,
            sha256: Sha256::digest(&built.zip)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            computers: bundle::summary(&built.computers),
        })
    }
}
