//! Step definitions for `features/v2_us5.feature`: administrator
//! applications in front on a Windows computer.

use std::fmt;
use std::time::Duration;

use cucumber::{World, given, then, when};
use mouser_core::EngineEvent;
use mouser_platform::{
    Capability, ForegroundApp, Limitation, LocalInput, Os, PlatformInfo, SessionType,
};
use mouser_proto::control::{FixAction, ProblemKind};
use mouser_proto::wire::ComputerMode;
use mouser_testkit::{Injected, VirtualPlatform};

use crate::support::desk::{Lab, Setup};

const WAIT: Duration = Duration::from_secs(20);

const ESC: u32 = 0x0007_0029;
const A: u32 = 0x0007_0004;
const CTRL: u32 = 0x0007_00E0;
const ALT: u32 = 0x0007_00E2;
const SHIFT: u32 = 0x0007_00E1;
const J: u32 = 0x0007_000D;

const APP: &str = "regedit.exe";

const NOT_ELEVATED: &str = "Portable Mouser can't control the login screen, permission prompts, Ctrl-Alt-Del, or applications running as administrator.";

#[derive(World)]
#[world(init = Self::new)]
pub struct ElevationWorld {
    pub lab: Lab,
}

impl fmt::Debug for ElevationWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ElevationWorld({:?})", self.lab.names())
    }
}

fn setup(os: Os, mode: ComputerMode, portable: bool, limitations: Vec<Limitation>) -> Setup {
    let platform = VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os,
        version: "test".into(),
        session: SessionType::Native,
        limitations,
    });
    Setup {
        platform,
        mouser_version: "0.3.0".into(),
        portable,
        mode: Some(mode),
        ..Setup::default()
    }
}

/// The application in front. Mouser outranked by it is what UIPI blocks.
fn app(outranks_mouser: bool) -> ForegroundApp {
    ForegroundApp {
        name: APP.into(),
        admin: true,
        outranks_mouser,
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = std::time::Instant::now() + WAIT;
    while !done() {
        assert!(
            std::time::Instant::now() < end,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn motion(dx: f64, abs: (f64, f64)) -> LocalInput {
    LocalInput::Motion { dx, dy: 0.0, abs }
}

fn tap(platform: &VirtualPlatform, usage: u32) {
    platform.send_input(LocalInput::Key { usage, down: true });
    platform.send_input(LocalInput::Key { usage, down: false });
}

fn motions(platform: &VirtualPlatform) -> usize {
    platform
        .injected()
        .iter()
        .filter(|i| matches!(i, Injected::MotionAbs { .. }))
        .count()
}

impl ElevationWorld {
    fn new() -> Self {
        Self { lab: Lab::new() }
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    /// Alpha (Linux) and Beta (Windows, running as `mode`) on one desk, with
    /// the application in front on Beta.
    async fn desk(&mut self, mode: ComputerMode, portable: bool, outranks: bool) {
        self.lab
            .form_desk(
                "Home office",
                "Alpha",
                &["Beta"],
                vec![
                    (
                        "Alpha",
                        setup(Os::Linux, ComputerMode::Installed, false, Vec::new()),
                    ),
                    ("Beta", setup(Os::Windows, mode, portable, Vec::new())),
                ],
            )
            .await;
        self.platform("Beta")
            .set_foreground_app(Some(app(outranks)));
    }

    /// Alpha's pointer crosses onto Beta.
    async fn alpha_to_beta(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        let before = motions(&beta);
        alpha.send_input(motion(1.0, (1919.0, 540.0)));
        until("Beta to have the pointer", || motions(&beta) > before).await;
    }

    fn quickjump_shows(&mut self, name: &str) -> usize {
        self.lab
            .node(name)
            .events
            .engine_events()
            .iter()
            .filter(|e| matches!(e, EngineEvent::QuickJumpShow { .. }))
            .count()
    }

    /// With the desk user at Beta: past its left edge, the panic key and the
    /// QuickJump hotkey behave as with any other application in front.
    async fn at_beta(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        // Past the edge: the pointer goes to Alpha.
        let before = motions(&alpha);
        beta.send_input(motion(-1.0, (0.0, 540.0)));
        until("Alpha to have the pointer", || motions(&alpha) > before).await;
        until("Beta to give it up", || beta.is_grabbed()).await;
        // The panic key brings it back.
        tap(&beta, ESC);
        tap(&beta, ESC);
        until("the panic key to bring the pointer back", || {
            !beta.is_grabbed()
        })
        .await;
        // A hotkey.
        let shown = self.quickjump_shows("Beta");
        for usage in [CTRL, ALT, SHIFT] {
            beta.send_input(LocalInput::Key { usage, down: true });
        }
        tap(&beta, J);
        until("QuickJump to show its letters", || {
            self.quickjump_shows("Beta") > shown
        })
        .await;
    }
}

// ---- scenarios 4, 5 and 7: input arrives ----------------------------------------------------------

#[given(
    "Mouser is installed on a Windows computer and an application running as administrator is in front"
)]
async fn installed_with_admin_app(w: &mut ElevationWorld) {
    w.desk(ComputerMode::Installed, false, false).await;
}

#[when("the desk user crosses to it and types, clicks, scrolls and moves the pointer")]
async fn crosses_and_works(w: &mut ElevationWorld) {
    w.alpha_to_beta().await;
    let alpha = w.platform("Alpha");
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: false,
    });
    alpha.send_input(LocalInput::Button {
        button: 1,
        down: true,
    });
    alpha.send_input(LocalInput::Button {
        button: 1,
        down: false,
    });
    alpha.send_input(LocalInput::Scroll { dx: 0.0, dy: 3.0 });
    alpha.send_input(motion(5.0, (1919.0, 540.0)));
}

#[then("the application receives that input")]
async fn application_receives_input(w: &mut ElevationWorld) {
    let beta = w.platform("Beta");
    until("every kind of input on Beta", || {
        let all = beta.injected();
        all.contains(&Injected::Key {
            usage: A,
            down: true,
        }) && all.contains(&Injected::Key {
            usage: A,
            down: false,
        }) && all.contains(&Injected::Button {
            button: 1,
            down: true,
        }) && all.contains(&Injected::Button {
            button: 1,
            down: false,
        }) && all
            .iter()
            .any(|i| matches!(i, Injected::Scroll { dy, .. } if *dy == 3.0))
            && motions(&beta) >= 2
    })
    .await;
    assert!(
        w.platform("Alpha").is_grabbed(),
        "Alpha still controls Beta"
    );
}

#[given(
    "Mouser is installed on a Windows computer the desk user is physically at and an application running as administrator is in front"
)]
async fn installed_and_physically_at(w: &mut ElevationWorld) {
    w.desk(ComputerMode::Installed, false, false).await;
}

#[when("they move the pointer past an edge, press the panic key or press a Mouser hotkey")]
async fn edge_panic_hotkey(w: &mut ElevationWorld) {
    w.at_beta().await;
}

#[then("it behaves as with any other application in front")]
async fn behaves_as_usual(w: &mut ElevationWorld) {
    // `at_beta` waited for each of the three; nothing was refused.
    assert_eq!(w.quickjump_shows("Beta"), 1);
    assert!(!w.platform("Beta").is_grabbed());
}

#[given("the desk owner chose \"Restart Mouser as administrator\" and approved Windows' prompt")]
async fn restarted_elevated(w: &mut ElevationWorld) {
    w.desk(ComputerMode::PortableElevated, true, false).await;
}

#[when(
    "the desk user crosses to that Windows computer with an application running as administrator in front"
)]
async fn crosses_to_elevated(w: &mut ElevationWorld) {
    w.alpha_to_beta().await;
    let alpha = w.platform("Alpha");
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: false,
    });
}

#[then(
    "the application receives their input, and moving past an edge, the panic key and Mouser hotkeys work there"
)]
async fn elevated_works(w: &mut ElevationWorld) {
    let beta = w.platform("Beta");
    until("the key on Beta", || {
        beta.injected().contains(&Injected::Key {
            usage: A,
            down: true,
        })
    })
    .await;
    // Control stays with Alpha for the length of a few ticks.
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert!(w.platform("Alpha").is_grabbed());
    w.lab
        .node("Alpha")
        .wait_status("no problem", |s| s.problems.is_empty())
        .await;
    // Back to Alpha, then the three behaviours at Beta.
    let alpha = w.platform("Alpha");
    tap(&alpha, ESC);
    tap(&alpha, ESC);
    until("Alpha to be itself again", || !alpha.is_grabbed()).await;
    w.at_beta().await;
}

// ---- scenario 6: portable returns the pointer ------------------------------------------------------

#[given(
    "portable Mouser runs on a Windows computer that has the pointer from another desk computer"
)]
async fn portable_has_the_pointer(w: &mut ElevationWorld) {
    w.desk(ComputerMode::Portable, true, false).await;
    w.alpha_to_beta().await;
    let alpha = w.platform("Alpha");
    alpha.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
}

#[when("an application running as administrator comes to the front there")]
async fn admin_app_comes_to_the_front(w: &mut ElevationWorld) {
    w.platform("Beta").set_foreground_app(Some(app(true)));
}

#[then(
    "the pointer and keyboard return to the computer they came from, and that computer shows a message naming the application and the Windows computer, saying portable Mouser cannot control applications running as administrator, and offering \"Restart Mouser as administrator\" as one action, with installing Mouser named as the way to also control the login screen, elevation prompts and Ctrl-Alt-Del"
)]
async fn returns_and_explains(w: &mut ElevationWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    until("the pointer to come back to Alpha", || !alpha.is_grabbed()).await;
    until("Beta to release the key", || {
        beta.injected().contains(&Injected::Key {
            usage: A,
            down: false,
        })
    })
    .await;
    let beta_id = w.lab.node("Beta").id();
    let status = w
        .lab
        .node("Alpha")
        .wait_status("the explanation", |s| {
            s.problems
                .iter()
                .any(|p| p.kind == ProblemKind::ElevatedApp)
        })
        .await;
    let problems: Vec<_> = status
        .problems
        .iter()
        .filter(|p| p.kind == ProblemKind::ElevatedApp)
        .collect();
    assert_eq!(problems.len(), 1);
    let problem = problems[0];
    assert_eq!(problem.computer, beta_id);
    assert_eq!(problem.app.as_deref(), Some(APP));
    assert_eq!(
        problem.message,
        "Portable Mouser on Beta can't control regedit.exe because it runs as administrator. \
         Restart Mouser as administrator to control it, or install Mouser to also control the \
         login screen, permission prompts and Ctrl-Alt-Del."
    );
    assert_eq!(problem.fix, Some(FixAction::RestartElevated(beta_id)));
    // Crossing again with the application still in front explains nothing twice.
    w.alpha_to_beta().await;
    until("the pointer to come back again", || !alpha.is_grabbed()).await;
    let status = w.lab.node("Alpha").status().await;
    assert_eq!(
        status
            .problems
            .iter()
            .filter(|p| p.kind == ProblemKind::ElevatedApp)
            .count(),
        1
    );
}

// ---- scenario 8: limitations in Health ------------------------------------------------------------------

#[given("portable Mouser runs on a Windows computer")]
async fn portable_on_windows(w: &mut ElevationWorld) {
    w.lab
        .form_desk(
            "Home office",
            "Alpha",
            &["Beta"],
            vec![
                (
                    "Alpha",
                    setup(Os::Linux, ComputerMode::Installed, false, Vec::new()),
                ),
                (
                    "Beta",
                    setup(
                        Os::Windows,
                        ComputerMode::Portable,
                        false,
                        vec![Limitation {
                            capability: Capability::SecureAttention,
                            text: NOT_ELEVATED.into(),
                        }],
                    ),
                ),
            ],
        )
        .await;
}

#[when("the desk owner opens its Health entry")]
async fn opens_health(_: &mut ElevationWorld) {
    // Health renders `computers[].limitations` of that computer; the next
    // step reads them where it does, on the computer and on a desk-mate.
}

#[then(
    "\"What doesn't work here\" states that portable mode cannot control the login screen, permission prompts or Ctrl-Alt-Del, and, unless Mouser was restarted as administrator, applications running as administrator"
)]
async fn health_lists_the_limitation(w: &mut ElevationWorld) {
    let beta = w.lab.node("Beta").id();
    for name in ["Alpha", "Beta"] {
        let status = w
            .lab
            .node(name)
            .wait_status("Beta's limitations", |s| {
                s.computers
                    .iter()
                    .any(|c| c.id == beta && c.limitations.iter().any(|l| l == NOT_ELEVATED))
            })
            .await;
        assert_eq!(status.problems, []);
        let others: Vec<_> = status
            .computers
            .iter()
            .filter(|c| c.id != beta)
            .flat_map(|c| &c.limitations)
            .collect();
        assert!(
            others.is_empty(),
            "{name}: only Beta lists them: {others:?}"
        );
    }
}
