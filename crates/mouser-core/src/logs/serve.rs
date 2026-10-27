//! Answering a desk-mate's `LogsRequest` (wire-protocol-delta "Logs
//! stream"): one unidirectional stream with this computer's card, settings
//! file and log files.

use std::sync::Arc;

use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{LogsCard, LogsPart, logs_stream_id};

use crate::config::FILE as CONFIG;
use crate::engine::Ctx;
use crate::lock::LockExt;

impl Ctx {
    /// What this computer sends of itself: the header and the bytes of each
    /// file. With logging off there are no files.
    pub(crate) fn logs_part(&self) -> (LogsPart, Vec<Vec<u8>>) {
        let info = self.platform.info();
        let logging = self.log.enabled();
        let files = if logging {
            self.log.snapshot()
        } else {
            Vec::new()
        };
        let part = LogsPart {
            card: LogsCard {
                id: self.me(),
                name: self.display_name(),
                os: crate::platform_os(info.os),
                os_version: info.version,
                version: self.mouser_version.clone(),
                mode: self.mode,
            },
            config_toml: std::fs::read_to_string(self.config_dir.join(CONFIG))
                .expect("config.toml is readable"),
            logging,
            files: files
                .iter()
                .map(|(name, bytes)| (name.clone(), bytes.len() as u64))
                .collect(),
        };
        (part, files.into_iter().map(|(_, bytes)| bytes).collect())
    }

    /// `peer` asks for our logs: answer on a stream of our own. Only desk
    /// members are answered.
    pub(crate) fn on_logs_request(
        self: &Arc<Self>,
        peer: ComputerId,
        request: [u8; 16],
    ) -> Result<(), String> {
        let core = self.core.guard();
        if !Self::authed(&core, peer) {
            return Err("LogsRequest from a computer that is not on the desk".into());
        }
        let conn = core.peers[&peer].conn.clone();
        drop(core);
        let (part, files) = self.logs_part();
        self.spawn(async move {
            // A connection lost meanwhile is the asker's to notice.
            let _ = stream_part(conn, logs_stream_id(&request), part, files).await;
        });
        Ok(())
    }
}

/// The stream id, the length of the header, the header, and then each file.
async fn stream_part(
    conn: quinn::Connection,
    id: [u8; 16],
    part: LogsPart,
    files: Vec<Vec<u8>>,
) -> std::io::Result<()> {
    let mut stream = conn.open_uni().await.map_err(std::io::Error::other)?;
    let header = postcard::to_stdvec(&part).expect("part serializes");
    let mut start = id.to_vec();
    start.extend((header.len() as u32).to_be_bytes());
    start.extend(header);
    stream.write_all(&start).await?;
    for bytes in files {
        stream.write_all(&bytes).await?;
    }
    stream.finish().map_err(std::io::Error::other)?;
    stream.stopped().await.map_err(std::io::Error::other)?;
    Ok(())
}
