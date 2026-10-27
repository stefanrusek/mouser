//! LAN distribution (Story 10 scenarios 1 and 3): the computer that holds a
//! verified release offers it to desk-mates that are older and serves their
//! `ReleaseFetch` requests; desk-mates never go to the internet for it.

use crate::lock::LockExt;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use mouser_proto::ids::ComputerId;
use mouser_proto::log::LogLevel;
use mouser_proto::release::ReleaseManifest;
use mouser_proto::wire::{ControlMessage, TransferId};
use sha2::{Digest, Sha256};

use super::{Offered, Version};
use crate::engine::{Core, Ctx, Phase};

/// The folder of verified releases, inside the config directory.
pub const FOLDER: &str = "releases";

const MANIFEST: &str = "manifest.json";

/// The scheme of the URLs a fetcher offers: its own endpoint on the desk.
const LAN_SCHEME: &str = "mouser-lan";

/// How long an offered desk-mate has to start its `ReleaseFetch` before the
/// fetcher stops counting on it (it may not update by itself).
pub const FETCH_START_GRACE: Duration = Duration::from_secs(5);

/// The longest the fetcher postpones its own restart for desk-mates that are
/// still fetching.
pub const FETCH_SETTLE_LIMIT: Duration = Duration::from_secs(60);

/// What the fetcher knows about a desk-mate it offered the release to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Serving {
    pub offered_at: tokio::time::Instant,
    pub fetching: bool,
}

/// A verified release stored on this computer: its manifest (only platforms
/// whose bundle verified) and the bundle files.
#[derive(Debug, Clone)]
pub struct Held {
    pub manifest: ReleaseManifest,
    dir: PathBuf,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Names derive from the text, so a published version or URL never decides
/// where a file goes.
fn name_of(text: &str) -> String {
    hex(&Sha256::digest(text.as_bytes())[..8])
}

impl Held {
    /// Store `bundles` (by URL) of `manifest` under `releases`, replacing
    /// every older stored release.
    pub(crate) fn store(
        releases: &Path,
        manifest: ReleaseManifest,
        bundles: &BTreeMap<String, Vec<u8>>,
    ) -> Held {
        let dir = releases.join(name_of(&manifest.version));
        if releases.exists() {
            std::fs::remove_dir_all(releases).expect("old releases removable");
        }
        std::fs::create_dir_all(&dir).expect("releases folder writable");
        for (url, bytes) in bundles {
            std::fs::write(dir.join(name_of(url)), bytes).expect("bundle writable");
        }
        let text = serde_json::to_string(&manifest).expect("manifest serializes");
        std::fs::write(dir.join(MANIFEST), text).expect("manifest writable");
        Held { manifest, dir }
    }

    /// The newest release stored under `releases`, if any.
    pub(crate) fn load(releases: &Path) -> Option<Held> {
        std::fs::read_dir(releases)
            .ok()?
            .filter_map(|entry| {
                let dir = entry.ok()?.path();
                let text = std::fs::read_to_string(dir.join(MANIFEST)).ok()?;
                let manifest: ReleaseManifest = serde_json::from_str(&text).ok()?;
                Some((
                    Version::parse(&manifest.version).ok()?,
                    Held { manifest, dir },
                ))
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, held)| held)
    }

    pub fn version(&self) -> Version {
        Version::parse(&self.manifest.version).expect("a stored release has a version")
    }

    /// The bundle for `platform` (a manifest key), if stored.
    pub fn bundle(&self, platform: &str) -> Option<Vec<u8>> {
        let asset = self.manifest.platforms.get(platform)?;
        std::fs::read(self.dir.join(name_of(&asset.url))).ok()
    }

    /// The manifest as `source` serves it: every URL points at its own
    /// endpoint on the desk, and only `ReleaseFetch` reaches it.
    pub fn lan_manifest(&self, source: ComputerId) -> String {
        let mut manifest = self.manifest.clone();
        for (platform, asset) in &mut manifest.platforms {
            asset.url = format!("{LAN_SCHEME}://{source}/{platform}");
        }
        serde_json::to_string(&manifest).expect("manifest serializes")
    }
}

/// The id the stream answering `ReleaseFetch{version, asset}` starts with.
pub fn stream_id(version: &str, asset: &str) -> TransferId {
    let digest = Sha256::digest(format!("release\n{version}\n{asset}"));
    TransferId(digest[..16].try_into().expect("16 bytes"))
}

impl Ctx {
    /// Offer the release we hold to `peer` if it runs an older version.
    pub(crate) fn offer_release(&self, core: &Core, peer: ComputerId) {
        let Some(held) = &core.release.held else {
            return;
        };
        let entry = &core.peers[&peer];
        let older = Version::parse(&entry.hello.mouser_version).is_ok_and(|v| v < held.version());
        if entry.phase == Phase::Authed && older {
            self.release_serving.guard().insert(
                peer,
                Serving {
                    offered_at: tokio::time::Instant::now(),
                    fetching: false,
                },
            );
            entry.send(ControlMessage::ReleaseAvailable {
                manifest_json: held.lan_manifest(self.me()),
            });
        }
    }

    /// Offer the release we hold to every older desk-mate.
    pub(crate) fn offer_release_to_all(&self, core: &Core) {
        for peer in core.peers.keys() {
            self.offer_release(core, *peer);
        }
    }

    /// A desk-mate offers a release: remember it if it is newer than ours
    /// and has a bundle for this platform, and update if that is automatic.
    pub(crate) fn on_release_available(
        self: &Arc<Self>,
        peer: ComputerId,
        manifest_json: &str,
    ) -> Result<(), String> {
        let manifest: ReleaseManifest = serde_json::from_str(manifest_json)
            .map_err(|e| format!("ReleaseAvailable with a malformed manifest: {e}"))?;
        let version = Version::parse(&manifest.version)?;
        let mut core = self.core.guard();
        if !Self::authed(&core, peer) {
            return Err("ReleaseAvailable from a computer that is not on the desk".into());
        }
        let wanted = version > Version::parse(&self.mouser_version).expect("own version is valid")
            && manifest.platforms.contains_key(&self.platform_key())
            && core.release.offered.as_ref().is_none_or(|o| {
                Version::parse(&o.manifest.version).is_ok_and(|current| {
                    // The same release offered again (a fetch that was cut
                    // short, the fetcher back after a restart) is a retry.
                    current < version || (current == version && !core.release.applying)
                })
            });
        if !wanted {
            return Ok(());
        }
        self.log.record(
            LogLevel::Info,
            "update.available",
            Some(peer),
            serde_json::json!({"version": manifest.version})
                .as_object()
                .cloned(),
        );
        core.release.offered = Some(Offered {
            from: peer,
            manifest,
        });
        let automatic = core.config.updates.automatic;
        drop(core);
        if automatic {
            self.spawn_apply();
        }
        Ok(())
    }

    /// A desk-mate asks for a bundle: stream it (nothing if we do not hold
    /// that release any more, which the asker takes as not verifying).
    pub(crate) fn on_release_fetch(
        self: &Arc<Self>,
        peer: ComputerId,
        version: String,
        asset: String,
    ) -> Result<(), String> {
        let core = self.core.guard();
        if !Self::authed(&core, peer) {
            return Err("ReleaseFetch from a computer that is not on the desk".into());
        }
        let bytes = core
            .release
            .held
            .as_ref()
            .filter(|held| held.manifest.version == version)
            .and_then(|held| held.bundle(&asset))
            .unwrap_or_default();
        let conn = core.peers[&peer].conn.clone();
        drop(core);
        if let Some(serving) = self.release_serving.guard().get_mut(&peer) {
            serving.fetching = true;
        }
        let ctx = self.clone();
        self.spawn(async move {
            // A connection lost meanwhile is the asker's to notice.
            let _ = stream_bundle(conn, stream_id(&version, &asset), bytes).await;
            ctx.release_serving.guard().remove(&peer);
        });
        Ok(())
    }

    /// Wait for the desk-mates offered the release to finish fetching it
    /// (or to go away), so that restarting does not cut their fetch short.
    pub(crate) async fn settle_fetches(&self) {
        self.settle_fetches_within(FETCH_START_GRACE, FETCH_SETTLE_LIMIT)
            .await;
    }

    pub(crate) async fn settle_fetches_within(&self, grace: Duration, limit: Duration) {
        let end = tokio::time::Instant::now() + limit;
        while tokio::time::Instant::now() < end
            && self
                .release_serving
                .guard()
                .values()
                .any(|s| s.fetching || s.offered_at.elapsed() < grace)
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

async fn stream_bundle(
    conn: quinn::Connection,
    id: TransferId,
    bytes: Vec<u8>,
) -> std::io::Result<()> {
    let mut stream = conn.open_uni().await.map_err(std::io::Error::other)?;
    let mut header = id.0.to_vec();
    header.extend(0u32.to_be_bytes());
    stream.write_all(&header).await?;
    stream.write_all(&bytes).await?;
    stream.finish().map_err(std::io::Error::other)?;
    stream.stopped().await.map_err(std::io::Error::other)?;
    Ok(())
}
