//! `update.check` and `update.apply` (Story 10): which version is running and
//! which is available, and installing the verified bundle through the app's
//! installer. The elected computer also looks for releases by itself.

use std::sync::Arc;
use std::time::Duration;

use mouser_proto::control::{RpcError, UpdateResult};
use mouser_proto::ids::ComputerId;
use mouser_proto::log::LogLevel;
use mouser_proto::wire::ControlMessage;
use tokio::io::AsyncReadExt;
use tokio::sync::oneshot;

use crate::engine::Ctx;
use crate::release::serve::{FOLDER, stream_id};
use crate::release::{Bundle, CheckSchedule, Offered, Version, fetch, minisign};

/// The longest a desk-mate may take to start sending a bundle.
const FETCH_TIMEOUT: Duration = Duration::from_secs(600);

/// The biggest bundle taken from a desk-mate.
const MAX_BUNDLE: usize = 1 << 30;

fn failed(message: impl Into<String>) -> RpcError {
    RpcError {
        code: -32000,
        message: message.into(),
        data: None,
    }
}

impl Ctx {
    /// The key of this computer's bundle in a release manifest.
    pub(crate) fn platform_key(&self) -> String {
        crate::release::platform_key(self.platform.info().os, std::env::consts::ARCH)
    }

    /// The running version and the newest one this computer could update to.
    pub(crate) fn update_result(&self) -> UpdateResult {
        UpdateResult {
            current: self.mouser_version.clone(),
            available: self
                .core
                .lock()
                .unwrap()
                .release
                .offered
                .as_ref()
                .map(|o| o.manifest.version.clone()),
        }
    }

    /// `update.check`: the elected computer looks at the release location
    /// first; the others only report what they were offered.
    pub(crate) async fn update_check(self: &Arc<Self>) -> Result<UpdateResult, RpcError> {
        let fetcher = self.core.lock().unwrap().is_fetcher(self.me());
        if fetcher {
            self.check_release().await.map_err(failed)?;
        }
        Ok(self.update_result())
    }

    /// `update.apply`: install the offered release.
    pub(crate) async fn update_apply(self: &Arc<Self>) -> Result<UpdateResult, RpcError> {
        self.apply_release().await.map_err(failed)?;
        Ok(self.update_result())
    }

    /// Look at the release location; keep, and offer the desk, a newer
    /// release that verifies.
    pub(crate) async fn check_release(self: &Arc<Self>) -> Result<(), String> {
        let (url, newer_than) = {
            let core = self.core.lock().unwrap();
            let mine = Version::parse(&self.mouser_version).expect("own version is valid");
            let held = core.release.held.as_ref().map(|h| h.version());
            (
                core.config.updates.release_url.clone(),
                held.map_or(mine.clone(), |held| held.max(mine)),
            )
        };
        let folder = self.config_dir.join(FOLDER);
        let Some(held) =
            fetch::check(&self.http, &url, &self.release_pubkey, &newer_than, &folder).await?
        else {
            return Ok(());
        };
        self.log.record(
            LogLevel::Info,
            "update.available",
            None,
            serde_json::json!({"version": held.manifest.version})
                .as_object()
                .cloned(),
        );
        let mut core = self.core.lock().unwrap();
        core.release.offered = Some(Offered {
            from: self.me(),
            manifest: held.manifest.clone(),
        });
        core.release.held = Some(held);
        self.offer_release_to_all(&core);
        let automatic = core.config.updates.automatic;
        drop(core);
        if automatic {
            self.spawn_apply();
        }
        Ok(())
    }

    /// Install the offered release in the background (automatic updates).
    pub(crate) fn spawn_apply(self: &Arc<Self>) {
        let ctx = self.clone();
        self.spawn(async move {
            if let Err(message) = ctx.apply_release().await {
                ctx.log.record(
                    LogLevel::Warn,
                    "update.failed",
                    None,
                    serde_json::json!({"message": message}).as_object().cloned(),
                );
            }
        });
    }

    /// Get this platform's bundle of the offered release, verify it against
    /// the updater key and hand it to the installer.
    pub(crate) async fn apply_release(self: &Arc<Self>) -> Result<(), String> {
        let (offered, installer) = {
            let mut core = self.core.lock().unwrap();
            if core.release.applying {
                return Err("an update is already being installed".into());
            }
            let offered = core
                .release
                .offered
                .clone()
                .ok_or("no update is available")?;
            let installer = self
                .installer
                .clone()
                .ok_or("this copy of Mouser cannot install updates")?;
            core.release.applying = true;
            (offered, installer)
        };
        let result = self.install(&offered, &installer).await;
        self.core.lock().unwrap().release.applying = false;
        result
    }

    async fn install(
        self: &Arc<Self>,
        offered: &Offered,
        installer: &crate::release::Installer,
    ) -> Result<(), String> {
        let key = self.platform_key();
        let version = offered.manifest.version.clone();
        let asset = offered
            .manifest
            .platforms
            .get(&key)
            .ok_or("no bundle for this computer")?;
        let bytes = if offered.from == self.me() {
            let core = self.core.lock().unwrap();
            let held = core
                .release
                .held
                .as_ref()
                .ok_or("the release is no longer stored")?;
            held.bundle(&key).ok_or("the bundle is no longer stored")?
        } else {
            self.fetch_bundle(offered.from, &version, &key).await?
        };
        minisign::verify(&self.release_pubkey, &asset.signature, &bytes)
            .map_err(|e| format!("the bundle was not installed: {e}"))?;
        installer(Bundle { version, bytes })
    }

    /// Ask `from` for a bundle over the desk connection.
    async fn fetch_bundle(
        &self,
        from: ComputerId,
        version: &str,
        asset: &str,
    ) -> Result<Vec<u8>, String> {
        let id = stream_id(version, asset);
        let (tx, rx) = oneshot::channel();
        {
            let core = self.core.lock().unwrap();
            if !Self::authed(&core, from) {
                return Err("the computer with the update is not connected".into());
            }
            self.release_waiters.lock().unwrap().push((id, from, tx));
            core.peers[&from].send(ControlMessage::ReleaseFetch {
                version: version.into(),
                asset: asset.into(),
            });
        }
        // Unanswered, or the connection ended and dropped the waiter.
        let answer = tokio::time::timeout(FETCH_TIMEOUT, rx).await;
        let stream = answer
            .ok()
            .and_then(Result::ok)
            .ok_or("the computer with the update did not answer")?;
        let mut bytes = Vec::new();
        // A stream cut short leaves a bundle that does not verify.
        let _ = stream.take(MAX_BUNDLE as u64).read_to_end(&mut bytes).await;
        Ok(bytes)
    }

    /// Hand `stream` to the `ReleaseFetch` it answers; if it answers none,
    /// it is given back.
    pub(crate) fn deliver_release(
        &self,
        id: &mouser_proto::wire::TransferId,
        stream: quinn::RecvStream,
    ) -> Option<quinn::RecvStream> {
        let mut waiters = self.release_waiters.lock().unwrap();
        let Some(at) = waiters.iter().position(|(waiting, _, _)| waiting == id) else {
            return Some(stream);
        };
        let (_, _, tx) = waiters.remove(at);
        // An asker that gave up has no use for the answer.
        let _ = tx.send(stream);
        None
    }

    /// Stop waiting for answers from `peer`, whose connection ended.
    pub(crate) fn drop_release_waiters(&self, peer: ComputerId) {
        self.release_waiters
            .lock()
            .unwrap()
            .retain(|(_, from, _)| *from != peer);
    }
}

/// Look for releases on `schedule` while the config says to and this
/// computer is the elected one.
pub(crate) async fn check_loop(ctx: Arc<Ctx>, schedule: CheckSchedule) {
    let mut tick =
        tokio::time::interval_at(tokio::time::Instant::now() + schedule.first, schedule.every);
    loop {
        tick.tick().await;
        let due = {
            let core = ctx.core.lock().unwrap();
            core.config.updates.automatic && core.is_fetcher(ctx.me())
        };
        if due && let Err(message) = ctx.check_release().await {
            ctx.log.record(
                LogLevel::Warn,
                "update.check_failed",
                None,
                serde_json::json!({"message": message}).as_object().cloned(),
            );
        }
    }
}
