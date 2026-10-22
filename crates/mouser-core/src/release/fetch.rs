//! The release check: find the newest published release, and if it is newer
//! than what this desk runs, download every platform's bundle and verify it
//! against the compiled-in updater key (Story 10 scenarios 1 and 3).

use std::collections::BTreeMap;
use std::path::Path;

use mouser_proto::release::ReleaseManifest;
use serde_json::Value;

use super::Version;
use super::http::Http;
use super::minisign;
use super::serve::Held;

/// The asset of a GitHub release that holds the updater manifest.
const MANIFEST_ASSET: &str = "latest.json";

/// Where the manifest of `release` (a GitHub release object) can be
/// downloaded from.
fn manifest_url(release: &Value) -> Option<&str> {
    release["assets"]
        .as_array()?
        .iter()
        .find(|asset| asset["name"] == MANIFEST_ASSET)?["browser_download_url"]
        .as_str()
}

/// The text of the newest manifest `release_url` leads to. It may be
/// `latest.json` itself, a GitHub release (`.../releases/latest`) or a list
/// of releases (`.../releases`, which unlike the first two also lists
/// pre-releases), newest first.
async fn manifest_text(http: &Http, release_url: &str) -> Result<Vec<u8>, String> {
    let body = http.get(release_url).await?;
    let value: Value = serde_json::from_slice(&body)
        .map_err(|e| format!("{release_url} did not answer with JSON: {e}"))?;
    if value.get("platforms").is_some() {
        return Ok(body);
    }
    let url = match value.as_array() {
        Some(releases) => releases
            .iter()
            .filter(|release| release["draft"] != true)
            .find_map(manifest_url),
        None => manifest_url(&value),
    };
    let url = url.ok_or_else(|| format!("{release_url} offers no {MANIFEST_ASSET}"))?;
    http.get(url).await
}

/// Look for a release newer than `newer_than`. If there is one, download its
/// bundles, keep those that `pubkey` verifies, and store the release under
/// `releases`. A bundle that does not verify is dropped from the release, so
/// it is never offered.
pub async fn check(
    http: &Http,
    release_url: &str,
    pubkey: &str,
    newer_than: &Version,
    releases: &Path,
) -> Result<Option<Held>, String> {
    let text = manifest_text(http, release_url).await?;
    let mut manifest: ReleaseManifest =
        serde_json::from_slice(&text).map_err(|e| format!("not a release manifest: {e}"))?;
    if Version::parse(&manifest.version)? <= *newer_than {
        return Ok(None);
    }
    let mut bundles = BTreeMap::new();
    let mut verified = BTreeMap::new();
    for (platform, asset) in &manifest.platforms {
        if !bundles.contains_key(&asset.url) {
            bundles.insert(asset.url.clone(), http.get(&asset.url).await?);
        }
        if minisign::verify(pubkey, &asset.signature, &bundles[&asset.url]).is_ok() {
            verified.insert(platform.clone(), asset.clone());
        }
    }
    if verified.is_empty() {
        return Err(format!(
            "no bundle of {} could be verified",
            manifest.version
        ));
    }
    manifest.platforms = verified;
    bundles.retain(|url, _| manifest.platforms.values().any(|a| &a.url == url));
    Ok(Some(Held::store(releases, manifest, &bundles)))
}
