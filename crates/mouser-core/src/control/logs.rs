//! `logs.collect` (Story 2 scenario 10, contracts/control-api-delta.md):
//! one bundle of every desk computer's logs, written on this computer.

use std::path::Path;
use std::sync::Arc;

use chrono::Utc;
use mouser_proto::control::{BundleReason, LogsCollectParams, LogsCollectResult, RpcError};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ComputerMode;

use crate::control::{internal, invalid};
use crate::engine::Ctx;
use crate::lock::LockExt;
use crate::logging::events;
use crate::logs::bundle::{self, Computer};
use crate::paths::Env;

/// A bundle, built in memory.
pub(crate) struct Built {
    pub name: String,
    pub zip: Vec<u8>,
    pub computers: Vec<Computer>,
}

impl Ctx {
    /// Collect the desk's logs and make the zip.
    pub(crate) async fn build_bundle(self: &Arc<Self>) -> Built {
        let computers = self.collect_logs().await;
        let desk = self
            .core
            .guard()
            .state
            .desk
            .as_ref()
            .map(|d| (d.desk_id, d.name.clone()));
        let at = Utc::now();
        let zip = bundle::build(
            at,
            self.me(),
            desk.as_ref().map(|(id, name)| (*id, name.as_str())),
            &computers,
        );
        Built {
            name: bundle::file_name(desk.as_ref().map(|(_, name)| name.as_str()), at),
            zip,
            computers,
        }
    }

    /// `logs.collect`: the bundle goes to `dir`, or to the user's Downloads
    /// folder. The installed service writes none (owner decision OD-7).
    pub(crate) async fn logs_collect(
        self: &Arc<Self>,
        params: LogsCollectParams,
    ) -> Result<LogsCollectResult, RpcError> {
        if self.mode == ComputerMode::Service {
            return Err(invalid(
                "logs.collect: the installed Mouser service does not write to a chosen folder; use logs.collect_stream",
            ));
        }
        let built = self.build_bundle().await;
        let dir = bundle::location(
            params.dir.as_deref().map(Path::new),
            self.mode,
            &self.config_dir,
            &Env::from_process(),
        );
        let path = bundle::unique_path(&dir, &built.name);
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&path, &built.zip))
            .map_err(|e| internal(format!("The log bundle could not be written: {e}")))?;
        let missing: Vec<(ComputerId, BundleReason)> = built
            .computers
            .iter()
            .filter_map(|c| Some((c.id, c.reason?)))
            .collect();
        self.log.write(events::logs_collected(
            &path,
            built.computers.len() - missing.len(),
            &missing,
        ));
        Ok(LogsCollectResult {
            path: path.to_string_lossy().into_owned(),
            computers: bundle::summary(&built.computers),
        })
    }
}
