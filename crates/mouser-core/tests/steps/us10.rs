//! Step definitions for `features/us10.feature`: a release host on the
//! local machine, a desk of computers that can be updated, and an installer
//! that records what the app would install.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_core::release::{Bundle, Installer};
use mouser_platform::{PermissionKind, PermissionState};
use mouser_proto::control::{ConnectionState, Method, ProblemKind, UpdateResult};
use serde_json::json;

use crate::support::desk::{Lab, Setup};
use crate::support::release::{Host, Signer};

const WAIT: Duration = Duration::from_secs(20);
const NAMES: [&str; 3] = ["Alpha", "Beta", "Gamma"];

/// Who was handed which bundle to install.
type Installed = Arc<Mutex<Vec<(String, Bundle)>>>;

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct UpdateWorld {
    pub lab: Lab,
    host: Option<Host>,
    signer: Signer,
    installed: Installed,
    /// The published bundles by platform key.
    bundles: BTreeMap<String, Vec<u8>>,
    /// The computer that goes to the release location: the lowest id.
    elected: &'static str,
    /// Computers whose next install fails.
    failing: Arc<Mutex<Vec<String>>>,
    /// The computer whose first install was made to fail (it leaves
    /// `failing` as soon as that install has failed).
    failed: Option<String>,
}

impl fmt::Debug for UpdateWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UpdateWorld({:?})", self.lab.names())
    }
}

impl UpdateWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            host: None,
            signer: Signer::new(),
            installed: Installed::default(),
            bundles: BTreeMap::new(),
            elected: "",
            failing: Arc::default(),
            failed: None,
        }
    }

    fn host(&self) -> &Host {
        self.host.as_ref().expect("a release host is running")
    }

    /// How `name` runs: at 0.1.0 with its release location on the local
    /// host and an installer that records what it is given.
    fn setup(&self, name: &str, automatic: bool) -> Setup {
        let installed = self.installed.clone();
        let failing = self.failing.clone();
        let who = name.to_string();
        let installer: Installer = Arc::new(move |bundle| {
            let mut failing = failing.lock().unwrap();
            if let Some(at) = failing.iter().position(|f| *f == who) {
                failing.remove(at);
                return Err("the disk is full".into());
            }
            installed.lock().unwrap().push((who.clone(), bundle));
            Ok(())
        });
        let mut setup = Setup {
            installer: Some(installer),
            release_pubkey: Some(self.signer.pubkey()),
            mouser_version: "0.1.0".into(),
            ..Setup::default()
        };
        setup.config.updates.automatic = automatic;
        setup.config.updates.release_url = self.host().release_url();
        setup
    }

    /// Three computers on one desk at version 0.1.0, with 0.2.0 published.
    async fn desk(&mut self, automatic: bool) {
        self.host = Some(Host::start().await);
        self.bundles = self.host().publish(&self.signer, "0.2.0");
        let setups = NAMES
            .iter()
            .map(|name| (*name, self.setup(name, automatic)))
            .collect();
        self.lab
            .form_desk("Home office", "Alpha", &["Beta", "Gamma"], setups)
            .await;
        let mut ids: Vec<_> = NAMES
            .iter()
            .map(|name| (self.lab.node(name).id(), *name))
            .collect();
        ids.sort();
        self.elected = ids[0].1;
    }

    fn fetcher(&self) -> &'static str {
        self.elected
    }

    async fn installs(&self, count: usize) {
        let end = Instant::now() + WAIT;
        while self.installed.lock().unwrap().len() < count {
            assert!(
                Instant::now() < end,
                "timed out waiting for {count} installs"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Wait until every computer in `names` has been given the 0.2.0 bundle.
    /// The suite's installer does not restart anyone, so a computer still on
    /// 0.1.0 can be offered the release again and install it a second time:
    /// how many installs there are, and in which order, is not an outcome.
    async fn installed_by(&self, names: &[&str]) {
        let end = Instant::now() + WAIT;
        loop {
            let installed = self.installed.lock().unwrap().clone();
            let done = names.iter().all(|name| {
                installed
                    .iter()
                    .any(|(who, bundle)| who == name && bundle.version == "0.2.0")
            });
            if done {
                return;
            }
            assert!(
                Instant::now() < end,
                "{names:?} did not all install 0.2.0: {installed:?}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

#[given("one computer on the desk has a newer version than the others")]
async fn one_is_newer(world: &mut UpdateWorld) {
    world.desk(true).await;
    let fetcher = world.fetcher();
    let others: Vec<_> = NAMES.iter().filter(|n| **n != fetcher).collect();
    for other in &others {
        world.lab.node(other).stop().await;
    }
    // The computer downloads the release, installs it and starts again as
    // the new version.
    let _: UpdateResult = world
        .lab
        .node(fetcher)
        .call(Method::UpdateCheck, json!({}))
        .await;
    world.installs(1).await;
    world.lab.node(fetcher).set_version("0.2.0");
    world.lab.restart(fetcher).await;
    world.installed.lock().unwrap().clear();
}

#[when("the others notice it")]
async fn others_notice(world: &mut UpdateWorld) {
    let fetcher = world.fetcher();
    for other in NAMES.iter().filter(|n| **n != fetcher) {
        world.lab.restart(other).await;
    }
    world.installs(2).await;
}

#[then(
    "they update to that version without the owner visiting them, and the desk works afterwards"
)]
async fn they_update(world: &mut UpdateWorld) {
    let fetcher = world.fetcher();
    let installed = world.installed.lock().unwrap().clone();
    for other in NAMES.iter().filter(|n| **n != fetcher) {
        assert!(
            installed.iter().any(|(who, bundle)| who == other
                && bundle.version == "0.2.0"
                && bundle.bytes.starts_with(b"Mouser 0.2.0 for ")),
            "{other} was not given the 0.2.0 bundle: {installed:?}"
        );
    }
    // Having installed, they start again as 0.2.0 and are on the desk.
    for other in NAMES.iter().filter(|n| **n != fetcher) {
        world.lab.node(other).set_version("0.2.0");
        world.lab.restart(other).await;
    }
    for name in NAMES {
        world
            .lab
            .node(name)
            .wait_status("every computer back on the desk at 0.2.0", |s| {
                s.computers.len() == 3
                    && s.computers.iter().all(|c| {
                        c.version == "0.2.0"
                            && (c.connection == ConnectionState::SelfComputer
                                || c.connection == ConnectionState::Connected)
                    })
                    && !s
                        .problems
                        .iter()
                        .any(|p| p.kind == ProblemKind::IncompatibleVersion)
            })
            .await;
    }
}

#[given("a computer has been updated")]
async fn updated(world: &mut UpdateWorld) {
    world.desk(false).await;
    let name = world.fetcher();
    let node = world.lab.node(name);
    node.platform
        .set_permission(PermissionKind::Accessibility, PermissionState::Granted);
    node.platform
        .set_permission(PermissionKind::InputMonitoring, PermissionState::Granted);
    let _: UpdateResult = world
        .lab
        .node(name)
        .call(Method::UpdateCheck, json!({}))
        .await;
    let _: UpdateResult = world
        .lab
        .node(name)
        .call(Method::UpdateApply, json!({}))
        .await;
    world.installs(1).await;
    world.lab.node(name).stop().await;
    world.lab.node(name).set_version("0.2.0");
}

#[when("Mouser starts again")]
async fn starts_again(world: &mut UpdateWorld) {
    let name = world.fetcher();
    world.lab.restart(name).await;
}

#[then("the operating-system permissions granted before the update are still granted")]
async fn permissions_survive(world: &mut UpdateWorld) {
    let name = world.fetcher();
    let status = world
        .lab
        .node(name)
        .wait_status("Alpha to run the new version", |s| s.version == "0.2.0")
        .await;
    let me = status
        .computers
        .iter()
        .find(|c| c.connection == ConnectionState::SelfComputer)
        .expect("this computer is listed");
    for kind in [
        mouser_proto::model::PermissionKind::Accessibility,
        mouser_proto::model::PermissionKind::InputMonitoring,
    ] {
        assert_eq!(
            me.permissions[&kind],
            mouser_proto::model::PermissionState::Granted
        );
    }
    assert!(
        !status
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::MissingPermission),
        "{:?}",
        status.problems
    );
}

#[given("a newer release has been published")]
async fn published(world: &mut UpdateWorld) {
    world.desk(true).await;
}

#[when("a computer on the desk next checks for releases")]
async fn next_check(world: &mut UpdateWorld) {
    // Every computer takes its turn; only the elected one goes out.
    for name in NAMES {
        let _: UpdateResult = world
            .lab
            .node(name)
            .call(Method::UpdateCheck, json!({}))
            .await;
    }
}

#[then(
    "it downloads the release and the other desk computers update from it over the local network without downloading it themselves"
)]
async fn downloaded_once(world: &mut UpdateWorld) {
    world.installed_by(&NAMES).await;
    let installed = world.installed.lock().unwrap().clone();
    for (who, bundle) in &installed {
        assert_eq!(bundle.version, "0.2.0", "{who}: {installed:?}");
        assert!(bundle.bytes.starts_with(b"Mouser 0.2.0 for "), "{who}");
    }
    let requests = world.host().requests.lock().unwrap().clone();
    let count = |path: &str| requests.iter().filter(|r| r.as_str() == path).count();
    assert_eq!(count("/latest.json"), 1, "{requests:?}");
    for key in world.bundles.keys() {
        assert_eq!(count(&format!("/0.2.0/{key}")), 1, "{requests:?}");
    }
    assert_eq!(requests.len(), 1 + world.bundles.len(), "{requests:?}");
}

#[given("an update is available")]
async fn available(world: &mut UpdateWorld) {
    world.desk(false).await;
    let fetcher = world.fetcher();
    let _: UpdateResult = world
        .lab
        .node(fetcher)
        .call(Method::UpdateCheck, json!({}))
        .await;
}

#[when("the owner looks at the tray menu")]
async fn tray(world: &mut UpdateWorld) {
    // The tray reads `update.check` on its own computer.
    for name in NAMES {
        let node = world.lab.node(name);
        let end = Instant::now() + WAIT;
        loop {
            let result: UpdateResult = node.call(Method::UpdateCheck, json!({})).await;
            if result.available.is_some() {
                break;
            }
            assert!(Instant::now() < end, "{name} was never offered the release");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

#[then("it shows the current and available versions")]
async fn shows_versions(world: &mut UpdateWorld) {
    for name in NAMES {
        let result: UpdateResult = world
            .lab
            .node(name)
            .call(Method::UpdateCheck, json!({}))
            .await;
        assert_eq!(
            result,
            UpdateResult {
                current: "0.1.0".into(),
                available: Some("0.2.0".into()),
            },
            "{name}"
        );
    }
}

/// The computers other than the elected one, in name order.
fn others(world: &UpdateWorld) -> Vec<&'static str> {
    let fetcher = world.fetcher();
    NAMES.into_iter().filter(|n| *n != fetcher).collect()
}

#[given("a desk where every computer updates automatically")]
async fn automatic_desk(world: &mut UpdateWorld) {
    world.desk(true).await;
}

#[given(
    "a desk where every computer updates automatically and one other computer's first install fails"
)]
async fn failing_desk(world: &mut UpdateWorld) {
    world.desk(true).await;
    let failing = others(world)[0].to_string();
    world.failing.lock().unwrap().push(failing.clone());
    world.failed = Some(failing);
}

#[when("the elected computer finds the release and restarts as the new version")]
async fn fetcher_restarts(world: &mut UpdateWorld) {
    let fetcher = world.fetcher();
    let _: UpdateResult = world
        .lab
        .node(fetcher)
        .call(Method::UpdateCheck, json!({}))
        .await;
    // The failing computer's install is not recorded.
    let expected: Vec<_> = NAMES
        .into_iter()
        .filter(|name| world.failed.as_deref() != Some(*name))
        .collect();
    world.installed_by(&expected).await;
    world.lab.node(fetcher).set_version("0.2.0");
    world.lab.restart(fetcher).await;
}

#[then("every computer installed it, the elected one restarting only after the others fetched it")]
async fn others_first(world: &mut UpdateWorld) {
    // The fetcher waits until the others have fetched, not until they have
    // finished installing, so the order of the installs is not fixed.
    world.installed_by(&NAMES).await;
    let installed = world.installed.lock().unwrap().clone();
    assert!(
        installed
            .iter()
            .all(|(_, bundle)| bundle.version == "0.2.0"),
        "{installed:?}"
    );
}

#[then("the computer whose install failed updates without the owner visiting it")]
async fn failed_one_updates(world: &mut UpdateWorld) {
    let failed = others(world)[0];
    // Computers that did install stay on the old version in this suite (the
    // installer does not restart them), so they may be offered the release
    // again too: wait for this computer's own install.
    let end = Instant::now() + WAIT;
    loop {
        let installed = world.installed.lock().unwrap().clone();
        if installed
            .iter()
            .any(|(who, bundle)| who == failed && bundle.version == "0.2.0")
        {
            break;
        }
        assert!(
            Instant::now() < end,
            "{failed} never updated: {installed:?}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(world.failing.lock().unwrap().is_empty());
}

#[given("a desk where every computer has updated over the LAN")]
async fn updated_over_lan(world: &mut UpdateWorld) {
    world.desk(true).await;
    let fetcher = world.fetcher();
    let _: UpdateResult = world
        .lab
        .node(fetcher)
        .call(Method::UpdateCheck, json!({}))
        .await;
    world.installed_by(&NAMES).await;
    world.installed.lock().unwrap().clear();
}

#[when(
    "one that was not the elected computer restarts as the new version and an older one rejoins"
)]
async fn relay_restarts(world: &mut UpdateWorld) {
    let (fetcher, others) = (world.fetcher(), others(world));
    let (updated, older) = (others[0], others[1]);
    // Only the computer that updated over the LAN can offer the release.
    world.lab.node(fetcher).stop().await;
    world.lab.node(older).stop().await;
    world.lab.node(updated).set_version("0.2.0");
    world.lab.restart(updated).await;
    world.lab.restart(older).await;
}

#[then("the older one updates from it")]
async fn older_updates(world: &mut UpdateWorld) {
    let older = others(world)[1];
    world.installed_by(&[older]).await;
    let installed = world.installed.lock().unwrap().clone();
    assert!(
        installed
            .iter()
            .any(|(who, bundle)| *who == older && bundle.bytes.starts_with(b"Mouser 0.2.0 for ")),
        "{installed:?}"
    );
}
