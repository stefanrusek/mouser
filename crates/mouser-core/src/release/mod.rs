//! Releases (Story 10): the elected computer checks the published release
//! location and downloads and verifies every platform's bundle; the desk
//! computers fetch their own bundle from it over the desk connection.

pub mod elect;
pub mod fetch;
pub mod http;
pub mod minisign;
pub mod pubkey;
pub mod serve;

#[cfg(test)]
mod elect_tests;
#[cfg(test)]
mod fetch_tests;
#[cfg(test)]
mod http_tests;
#[cfg(test)]
mod minisign_tests;
#[cfg(test)]
mod serve_tests;
#[cfg(test)]
pub(crate) mod testing;

use std::cmp::Ordering;
use std::sync::Arc;

use mouser_proto::ids::ComputerId;
use mouser_proto::release::ReleaseManifest;

pub use serve::Held;

/// A verified update, handed to the application's installer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The release's version, as published.
    pub version: String,
    /// The updater bundle of this computer's platform.
    pub bytes: Vec<u8>,
}

/// The application's installer: installs the bundle and restarts Mouser.
pub type Installer = Arc<dyn Fn(Bundle) -> Result<(), String> + Send + Sync>;

/// When a computer looks for a new release, if it does so by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckSchedule {
    /// Delay before the first look, so that desk-mates have connected and
    /// only the elected computer goes to the internet.
    pub first: std::time::Duration,
    pub every: std::time::Duration,
}

impl CheckSchedule {
    /// What the app uses: a first look after 30 s, then every six hours.
    pub const DEFAULT: Self = Self {
        first: std::time::Duration::from_secs(30),
        every: std::time::Duration::from_secs(6 * 60 * 60),
    };
}

/// A release version, ordered as semantic versions are (a pre-release is
/// older than its release).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Option<String>,
}

impl Version {
    /// `1.2.3`, `v1.2.3` or `1.2.3-beta.1`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let bad = || format!("{text:?} is not a version");
        let text = text.strip_prefix('v').unwrap_or(text);
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_string())),
            None => (text, None),
        };
        let parts: Vec<u64> = core
            .split('.')
            .map(|p| p.parse().map_err(|_| bad()))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            core: parts.try_into().map_err(|_| bad())?,
            pre,
        })
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (&self.pre, &other.pre) {
                (None, None) => Ordering::Equal,
                (None, Some(_)) => Ordering::Greater,
                (Some(_), None) => Ordering::Less,
                (Some(a), Some(b)) => a.cmp(b),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The key of this computer's bundle in a manifest's `platforms`.
pub fn platform_key(os: mouser_platform::Os, arch: &str) -> String {
    let os = match os {
        mouser_platform::Os::MacOs => "darwin",
        mouser_platform::Os::Windows => "windows",
        mouser_platform::Os::Linux => "linux",
    };
    format!("{os}-{arch}")
}

/// A release offered to this computer by a desk-mate (or held by itself).
#[derive(Debug, Clone)]
pub(crate) struct Offered {
    /// Where the bundle comes from.
    pub from: ComputerId,
    pub manifest: ReleaseManifest,
}

/// What a computer knows about releases.
#[derive(Default)]
pub(crate) struct ReleaseState {
    /// The newest verified release stored on this computer.
    pub held: Option<Held>,
    /// The newest release this computer could update to.
    pub offered: Option<Offered>,
    /// An update is being fetched or installed.
    pub applying: bool,
}

impl ReleaseState {
    /// The state at start: the release stored by an earlier run, which is
    /// also an update for this computer if it is newer than `current`.
    pub(crate) fn load(folder: &std::path::Path, me: ComputerId, current: &Version) -> Self {
        let held = Held::load(folder);
        let offered = held
            .as_ref()
            .filter(|held| held.version() > *current)
            .map(|held| Offered {
                from: me,
                manifest: held.manifest.clone(),
            });
        Self {
            held,
            offered,
            applying: false,
        }
    }
}
