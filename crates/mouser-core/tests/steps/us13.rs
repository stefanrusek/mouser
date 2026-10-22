//! Step definitions for `features/us13.feature`: Mouser runs from a folder
//! or removable drive without installing anything on the computer.

use std::fmt;

use cucumber::{World, given, then, when};
use mouser_platform::{Os, Platform, PlatformInfo, SessionType};
use mouser_proto::control::{ConnectionState, ProblemKind, Status};
use mouser_proto::ids::ComputerId;
use mouser_testkit::VirtualPlatform;

use crate::support::desk::{Lab, Setup};

const WINDOWS_LIMITATION: &str = "Portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator.";

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct PortableWorld {
    pub lab: Lab,
    status: Option<Status>,
    beta: Option<ComputerId>,
}

impl fmt::Debug for PortableWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PortableWorld({:?})", self.lab.names())
    }
}

fn portable(os: Os) -> Setup {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os,
        version: "virtual".into(),
        session: SessionType::Native,
        limitations: Vec::new(),
    });
    Setup {
        platform,
        portable: true,
        ..Setup::default()
    }
}

impl PortableWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            status: None,
            beta: None,
        }
    }

    /// Alpha is installed; Beta runs from a drive. Both form a desk.
    async fn desk_with_portable_beta(&mut self) {
        self.lab
            .form_desk(
                "Home office",
                "Alpha",
                &["Beta"],
                vec![("Alpha", Setup::default()), ("Beta", portable(Os::Linux))],
            )
            .await;
    }

    /// The computer itself holds nothing of Mouser's but its control socket.
    fn assert_computer_keeps_nothing(&mut self) {
        let beta = self.lab.node("Beta");
        let left: Vec<_> = std::fs::read_dir(beta.dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|name| name != "mouser.sock")
            .collect();
        assert!(left.is_empty(), "left on the computer: {left:?}");
        assert!(
            !beta.platform.is_autostart_enabled(),
            "start at login was registered"
        );
        assert_eq!(
            beta.platform
                .secrets()
                .get(mouser_core::identity::DEVICE_KEY)
                .unwrap(),
            None,
            "a secret went to the computer's own store"
        );
    }
}

// ---- scenario 1: run from a removable drive ----------------------------------------------

#[given("the portable build is on a removable drive")]
async fn on_a_drive(_: &mut PortableWorld) {
    // The drive is made when the portable computer is first run.
}

#[when("the owner runs it on a computer")]
async fn runs_it(w: &mut PortableWorld) {
    w.desk_with_portable_beta().await;
}

#[then(
    "it joins a desk and behaves as a desk member without anything being installed on that computer"
)]
async fn joins_without_installing(w: &mut PortableWorld) {
    let alpha = w.lab.node("Alpha").id();
    let status = w.lab.node("Beta").status().await;
    assert!(
        status
            .computers
            .iter()
            .any(|c| c.id == alpha && c.connection == ConnectionState::Connected && c.displays > 0)
    );
    assert!(w.lab.node("Beta").drive_data().join("state.json").is_file());
    w.assert_computer_keeps_nothing();
}

// ---- scenario 2: settings stay on the drive ----------------------------------------------

#[given("the portable build has been run")]
async fn has_been_run(w: &mut PortableWorld) {
    w.desk_with_portable_beta().await;
}

#[when("the owner removes the drive")]
async fn removes_the_drive(w: &mut PortableWorld) {
    w.beta = Some(w.lab.node("Beta").id());
    w.lab.node("Beta").stop().await;
}

#[then(
    "the computer keeps no Mouser settings, and the settings are on the drive the next time it runs"
)]
async fn settings_are_on_the_drive(w: &mut PortableWorld) {
    w.assert_computer_keeps_nothing();
    let data = w.lab.node("Beta").drive_data();
    for file in ["config.toml", "state.json"] {
        assert!(data.join(file).is_file(), "{file} is not on the drive");
    }
    assert!(data.join("secrets").is_dir());
    // Next time it runs, the same computer is back on the desk.
    let beta = w.beta.expect("drive removed");
    w.lab.restart("Beta").await;
    assert_eq!(w.lab.node("Beta").id(), beta);
    w.lab
        .node("Alpha")
        .wait_status("Beta to be back on the desk", |s| {
            s.computers
                .iter()
                .any(|c| c.id == beta && c.connection == ConnectionState::Connected)
        })
        .await;
}

// ---- scenario 3: Windows portable limitation ---------------------------------------------

#[given(
    "portable mode on Windows cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator"
)]
async fn windows_portable(w: &mut PortableWorld) {
    w.lab.add_with("Beta", portable(Os::Windows)).await;
}

#[when("the owner looks at the app running in portable mode on Windows")]
async fn looks_at_the_app(w: &mut PortableWorld) {
    w.status = Some(w.lab.node("Beta").status().await);
}

#[then(
    "the app states that portable mode cannot control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator"
)]
async fn states_the_limitation(w: &mut PortableWorld) {
    let status = w.status.take().expect("status read");
    assert!(
        status
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::PlatformLimitation && p.message == WINDOWS_LIMITATION),
        "{:?}",
        status.problems
    );
    assert!(
        status.computers[0]
            .limitations
            .iter()
            .any(|l| l == WINDOWS_LIMITATION)
    );
}
