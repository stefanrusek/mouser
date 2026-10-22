//! Step definitions for `features/us12.feature`: the shared mouse and
//! keyboard reach a computer at its login or system security screen, and
//! Ctrl-Alt-Del can be sent to a Windows computer.

use std::fmt;

use cucumber::{World, given, then, when};
use mouser_platform::{LocalInput, Os, PlatformInfo, SessionType};
use mouser_proto::control::Method;
use mouser_testkit::{Injected, VirtualPlatform};

use crate::support::desk::{Lab, Setup};

const ENTER: u32 = 0x0007_0028;
const P: u32 = 0x0007_0013;
const W: u32 = 0x0007_001A;

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct LoginWorld {
    pub lab: Lab,
}

impl fmt::Debug for LoginWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LoginWorld({:?})", self.lab.names())
    }
}

/// A computer of operating system `os` that can be sent Ctrl-Alt-Del when
/// it is Windows.
fn computer(os: Os) -> Setup {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os,
        version: "virtual".into(),
        session: SessionType::X11,
        limitations: Vec::new(),
    });
    platform.set_secure_attention_supported(os == Os::Windows);
    Setup {
        platform,
        ..Setup::default()
    }
}

impl LoginWorld {
    fn new() -> Self {
        Self { lab: Lab::new() }
    }

    /// Alpha (Linux) and Beta (`os`) on one desk.
    async fn desk_with(&mut self, os: Os) {
        self.lab
            .form_desk(
                "Home office",
                "Alpha",
                &["Beta"],
                vec![("Alpha", Setup::default()), ("Beta", computer(os))],
            )
            .await;
    }

    /// Beta is at its login screen: only the headless engine runs there.
    async fn beta_at_login_screen(&mut self, os: Os) {
        self.desk_with(os).await;
        self.lab.go_to_login_screen("Beta").await;
        let beta = self.lab.node("Beta").id();
        self.lab
            .node("Alpha")
            .wait_status("Beta to be back on the desk", |s| {
                s.computers.iter().any(|c| {
                    c.id == beta
                        && c.connection == mouser_proto::control::ConnectionState::Connected
                        && c.displays > 0
                })
            })
            .await;
    }

    fn alpha(&mut self) -> VirtualPlatform {
        self.lab.node("Alpha").platform.clone()
    }

    fn beta(&mut self) -> VirtualPlatform {
        self.lab.node("Beta").platform.clone()
    }

    /// Push Alpha's pointer through its right edge.
    async fn cross_to_beta(&mut self) {
        self.alpha().send_input(LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (1919.0, 540.0),
        });
        let beta = self.lab.node("Beta").id();
        self.lab
            .node("Alpha")
            .wait_status("Beta to be the active computer", |s| {
                s.active_computer == Some(beta)
            })
            .await;
    }

    async fn beta_injected(&mut self, what: &str, pred: impl Fn(&Injected) -> bool) {
        let beta = self.beta();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !beta.injected().iter().any(&pred) {
            assert!(
                std::time::Instant::now() < end,
                "timed out waiting for {what}: {:?}",
                beta.injected()
            );
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    }

    /// Type `usages` on Alpha's keyboard.
    fn type_keys(&mut self, usages: &[u32]) {
        let alpha = self.alpha();
        for usage in usages {
            for down in [true, false] {
                alpha.send_input(LocalInput::Key {
                    usage: *usage,
                    down,
                });
            }
        }
    }

    /// Beta shows every key typed on Alpha pressed and released, and a click.
    async fn can_operate(&mut self) {
        self.type_keys(&[P, W, ENTER]);
        for usage in [P, W, ENTER] {
            for down in [true, false] {
                self.beta_injected("a typed key", |i| *i == Injected::Key { usage, down })
                    .await;
            }
        }
        let alpha = self.alpha();
        for down in [true, false] {
            alpha.send_input(LocalInput::Button { button: 1, down });
            self.beta_injected("a click", |i| *i == Injected::Button { button: 1, down })
                .await;
        }
    }
}

// ---- scenario 1: Mac login window --------------------------------------------------------

#[given("a Mac on the desk is at its login window")]
async fn mac_at_login_window(w: &mut LoginWorld) {
    w.beta_at_login_screen(Os::MacOs).await;
}

#[when("the desk user crosses to it")]
async fn crosses_to_it(w: &mut LoginWorld) {
    w.cross_to_beta().await;
}

#[then("they can type their password and log in")]
async fn can_log_in_on_the_mac(w: &mut LoginWorld) {
    w.can_operate().await;
}

// ---- scenario 2: Windows login after start -----------------------------------------------

#[given("a Windows computer has just started and shows its login screen")]
async fn windows_at_login_screen(w: &mut LoginWorld) {
    w.beta_at_login_screen(Os::Windows).await;
}

#[then("they can log in with the shared mouse and keyboard")]
async fn can_log_in_on_windows(w: &mut LoginWorld) {
    w.can_operate().await;
}

// ---- scenario 3: elevation prompt or user switching --------------------------------------

#[given("a Windows computer shows a permission elevation prompt or a user-switching screen")]
async fn windows_shows_a_secure_screen(w: &mut LoginWorld) {
    w.beta_at_login_screen(Os::Windows).await;
    w.beta().set_foreground_elevated(true);
}

#[then("they can operate that screen")]
async fn can_operate_the_screen(w: &mut LoginWorld) {
    w.can_operate().await;
}

// ---- scenario 4: Send Ctrl-Alt-Del -------------------------------------------------------

#[given("the active computer runs Windows")]
async fn active_computer_runs_windows(w: &mut LoginWorld) {
    w.desk_with(Os::Windows).await;
    w.cross_to_beta().await;
}

#[when("the desk user chooses Send Ctrl-Alt-Del from the tray menu")]
async fn chooses_send_cad(w: &mut LoginWorld) {
    // The tray menu calls the control API's `send_cad`.
    let _: mouser_proto::control::Empty = w
        .lab
        .node("Alpha")
        .call(Method::SendCad, serde_json::json!({}))
        .await;
}

#[then("that computer responds as if Ctrl-Alt-Del had been pressed on its own keyboard")]
async fn the_computer_responds(w: &mut LoginWorld) {
    w.beta_injected("the secure attention sequence", |i| {
        *i == Injected::SecureAttention
    })
    .await;
}
