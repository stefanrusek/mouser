//! Step definitions for `features/v2_us2.feature`: the diagnostic log shows
//! what Mouser did and why. Computers run Mouser 0.3.0 with logging on and a
//! 1 MiB limit, so that the log rotates after a few large records; every
//! record found is also checked against the closed catalog of
//! contracts/log-events.md.

use std::fmt;
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_platform::{CaptureLossReason, ForegroundApp, LocalDisplay, LocalInput};
use mouser_proto::control::{BundleReason, Empty, LoggingSetResult, LogsCollectResult, Method};
use mouser_testkit::{Injected, InputKind, VirtualPlatform};
use serde_json::{Value, json};

use crate::steps::us3::{push_back, push_toward, until};
use crate::support::desk::{Lab, Setup};
use crate::support::logs::{self, Zip};

const A: u32 = 0x0007_0004;
const C: u32 = 0x0007_0006;
const ESC: u32 = 0x0007_0029;
const CTRL: u32 = 0x0007_00E0;
const SHIFT: u32 = 0x0007_00E1;
const ALT: u32 = 0x0007_00E2;
const KEY_A: u32 = 0x0007_0004;
const KEY_B: u32 = 0x0007_0005;
const KEY_J: u32 = 0x0007_000D;

const WAIT: Duration = Duration::from_secs(20);

#[derive(World)]
#[world(init = Self::new)]
pub struct LogWorld {
    pub lab: Lab,
    /// The ids of the computers, remembered while they run.
    ids: std::collections::BTreeMap<String, String>,
    /// The first record of each log file the agent opened.
    opened: Vec<Value>,
    /// Where the bundle was written, and what `logs.collect` said.
    out: Option<tempfile::TempDir>,
    bundle: Option<LogsCollectResult>,
}

impl fmt::Debug for LogWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "LogWorld({:?})", self.lab.names())
    }
}

fn app(name: &str, admin: bool) -> ForegroundApp {
    ForegroundApp {
        name: name.into(),
        admin,
        outranks_mouser: false,
    }
}

fn setup(version: &str, logging: bool) -> Setup {
    let mut setup = Setup {
        mouser_version: version.into(),
        ..Setup::default()
    };
    setup.config.diagnostics.logging = logging;
    setup.config.diagnostics.max_log_mb = 1;
    setup
}

fn tap(platform: &VirtualPlatform, usage: u32) {
    for down in [true, false] {
        platform.send_input(LocalInput::Key { usage, down });
    }
}

fn entered(platform: &VirtualPlatform) -> bool {
    platform
        .injected()
        .iter()
        .any(|i| matches!(i, Injected::MotionAbs { .. }))
}

fn field<'a>(record: &'a Value, name: &str) -> &'a Value {
    &record["fields"][name]
}

impl LogWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            ids: std::collections::BTreeMap::new(),
            opened: Vec::new(),
            out: None,
            bundle: None,
        }
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    /// The id of `name`, which is remembered the first time it is asked
    /// for (the computer may have stopped by the time it is needed again).
    fn id(&mut self, name: &str) -> String {
        if !self.ids.contains_key(name) {
            let id = self.lab.node(name).id().to_string();
            self.ids.insert(name.to_string(), id);
        }
        self.ids[name].clone()
    }

    /// A desk of `names` (the first creates it) whose logging is on: it is
    /// turned on once the desk exists, so that the header names it.
    async fn desk(&mut self, names: &[&str]) {
        let setups = names.iter().map(|n| (*n, setup("0.3.0", false))).collect();
        self.lab
            .form_desk("Home office", names[0], &names[1..], setups)
            .await;
        for name in names {
            self.logging_on(name).await;
        }
    }

    async fn logging_on(&mut self, name: &str) {
        let _: LoggingSetResult = self
            .lab
            .node(name)
            .call(Method::LoggingSet, json!({"enabled": true}))
            .await;
    }

    /// Every record of `name`, oldest first.
    fn records(&mut self, name: &str) -> Vec<Value> {
        logs::records(self.lab.node(name))
    }

    /// Wait for a record of `event` at `name` that satisfies `pred`.
    async fn wait_for(
        &mut self,
        name: &str,
        event: &str,
        what: &str,
        pred: impl Fn(&Value) -> bool,
    ) -> Value {
        let end = Instant::now() + WAIT;
        loop {
            if let Some(found) = self
                .records(name)
                .into_iter()
                .find(|r| r["event"] == event && pred(r))
            {
                return found;
            }
            assert!(
                Instant::now() < end,
                "{name}: timed out waiting for {event}: {what}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Write large records until the log of `name` has rotated.
    async fn rotate(&mut self, name: &str) {
        let (_, older) = logs::files(self.lab.node(name));
        for n in 0..20 {
            if older.exists() {
                return;
            }
            let value = format!("https://example.test/{n}/{}", "x".repeat(150_000));
            let _: Empty = self
                .lab
                .node(name)
                .call(
                    Method::ConfigSet,
                    json!({"key": "updates.release_url", "value": value}),
                )
                .await;
        }
        assert!(older.exists(), "{name}: the log never rotated");
    }

    /// The first record of the current log file of `name`.
    fn first_record(&mut self, name: &str) -> Value {
        let (current, _) = logs::files(self.lab.node(name));
        logs::read(&current).remove(0)
    }

    /// The desk user pushes Alpha's shared mouse onto Beta and waits until
    /// Beta has the pointer.
    async fn cross_to_beta(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        beta.take_injected();
        push_toward("Alpha", &alpha);
        until("Beta to have the pointer", || {
            alpha.is_grabbed() && entered(&beta)
        })
        .await;
    }

    async fn back_to_alpha(&mut self) {
        let alpha = self.platform("Alpha");
        push_back("Alpha", &alpha);
        until("the pointer to be back on Alpha", || {
            !alpha.pointer_hidden()
        })
        .await;
    }

    /// Ctrl+Alt+Shift+J and then `key`.
    fn quickjump(&mut self, key: u32) {
        let alpha = self.platform("Alpha");
        for usage in [CTRL, ALT, SHIFT] {
            alpha.send_input(LocalInput::Key { usage, down: true });
        }
        tap(&alpha, KEY_J);
        for usage in [SHIFT, ALT, CTRL] {
            alpha.send_input(LocalInput::Key { usage, down: false });
        }
        tap(&alpha, key);
    }

    /// Wait for every computer to be connected to all the others.
    async fn linked(&mut self) {
        for name in ["Alpha", "Beta", "Gamma"] {
            self.lab
                .node(name)
                .wait_status("every computer connected", |s| {
                    s.computers
                        .iter()
                        .filter(|c| {
                            c.connection == mouser_proto::control::ConnectionState::Connected
                        })
                        .count()
                        == 2
                })
                .await;
        }
    }

    /// Every record in the log of `name` is in the closed catalog.
    fn check_catalog(&mut self, name: &str) {
        logs::assert_every_record_is_in_the_catalog(self.lab.node(name));
    }

    async fn collect(&mut self) {
        let out = tempfile::tempdir().unwrap();
        let dir = out.path().to_str().unwrap().to_string();
        let result: LogsCollectResult = self
            .lab
            .node("Alpha")
            .call(Method::LogsCollect, json!({ "dir": dir }))
            .await;
        self.out = Some(out);
        self.bundle = Some(result);
    }

    /// The zip the last `logs.collect` wrote.
    fn zip(&self) -> Zip {
        let path = &self.bundle.as_ref().expect("a bundle was collected").path;
        Zip::open(std::path::Path::new(path))
    }
}

// ---- Story 2 scenario 1 --------------------------------------------------------------------

#[given("logging is on")]
async fn logging_is_on(w: &mut LogWorld) {
    w.desk(&["Alpha", "Beta", "Gamma"]).await;
}

#[when("the agent opens any log file, including one started when the previous file was rotated")]
async fn opens_any_log_file(w: &mut LogWorld) {
    let first = w.first_record("Alpha");
    w.opened.push(first);
    w.rotate("Alpha").await;
    let after_rotation = w.first_record("Alpha");
    w.opened.push(after_rotation);
}

#[then(
    "its first record states the Mouser version, the operating system and its version, the computer's id and name, and the desk"
)]
async fn first_record_states_the_computer(w: &mut LogWorld) {
    assert_eq!(w.opened.len(), 2);
    let id = w.id("Alpha");
    for header in &w.opened {
        assert_eq!(header["event"], "log.header");
        assert_eq!(field(header, "mouser_version"), "0.3.0");
        assert!(["macos", "windows", "linux"].contains(&field(header, "os").as_str().unwrap()));
        assert!(!field(header, "os_version").as_str().unwrap().is_empty());
        assert_eq!(*field(header, "computer_id"), id);
        assert_eq!(field(header, "computer_name"), "Alpha");
        assert_eq!(field(header, "desk_name"), "Home office");
        assert_eq!(field(header, "desk_id").as_str().unwrap().len(), 32);
        assert_eq!(field(header, "mode"), "installed");
        assert!(
            field(header, "log_path")
                .as_str()
                .unwrap()
                .ends_with("mouser.log")
        );
    }
    w.check_catalog("Alpha");
}

// ---- Story 2 scenario 2 --------------------------------------------------------------------

#[when(
    regex = r"^control moves between computers \(a crossing, the panic key, QuickJump, a pause, or a lost link\)$"
)]
async fn control_moves(w: &mut LogWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    alpha.set_foreground_app(Some(app("Terminal", false)));
    beta.set_foreground_app(Some(app("regedit.exe", true)));
    // A crossing there and back.
    w.cross_to_beta().await;
    w.back_to_alpha().await;
    // The panic key.
    w.cross_to_beta().await;
    tap(&alpha, ESC);
    tap(&alpha, ESC);
    until("the panic key to bring the pointer back", || {
        !alpha.pointer_hidden()
    })
    .await;
    // A pause.
    w.cross_to_beta().await;
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActivePause, json!({}))
        .await;
    until("the pause to bring the pointer back", || {
        !alpha.pointer_hidden()
    })
    .await;
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(Method::ActiveResume, json!({}))
        .await;
    // QuickJump there and back.
    w.quickjump(KEY_B);
    until("QuickJump to take the pointer to Beta", || {
        alpha.is_grabbed()
    })
    .await;
    w.quickjump(KEY_A);
    until("QuickJump to bring the pointer back", || {
        !alpha.pointer_hidden()
    })
    .await;
    // A lost link.
    w.cross_to_beta().await;
    w.lab.node("Beta").sim.set_blackhole(true);
    until("the lost link to bring the pointer back", || {
        !alpha.pointer_hidden()
    })
    .await;
    w.lab.node("Beta").sim.set_blackhole(false);
    w.linked().await;
}

#[then(
    "a record names the computer control left and the computer it reached, how it moved, and the application in front on the computer that received control, including whether that application runs as administrator"
)]
async fn control_records(w: &mut LogWorld) {
    let (alpha, beta) = (w.id("Alpha"), w.id("Beta"));
    let at_alpha = w.records("Alpha");
    let moves = |how: &str, from: &str, to: &str| -> Vec<Value> {
        at_alpha
            .iter()
            .filter(|r| {
                r["event"] == "control.changed"
                    && *field(r, "how") == how
                    && *field(r, "from") == from
                    && *field(r, "to") == to
            })
            .cloned()
            .collect()
    };
    // Control left Alpha for Beta: how it moved, and nothing about an
    // application, which is Beta's to say.
    for how in ["crossing", "quickjump"] {
        let left = &moves(how, &alpha, &beta)[0];
        assert!(field(left, "app").is_null(), "{left}");
    }
    // Control reached Alpha: Alpha's application comes with it.
    for how in ["crossing", "panic", "pause", "quickjump", "link_lost"] {
        let reached = moves(how, &beta, &alpha);
        assert!(!reached.is_empty(), "{how}");
        assert_eq!(field(&reached[0], "app"), "Terminal", "{how}");
        assert_eq!(field(&reached[0], "app_admin"), false, "{how}");
    }
    // Control reached Beta: Beta's application, which runs as administrator.
    let entered = w
        .wait_for("Beta", "control.changed", "entered", |r| {
            *field(r, "how") == "entered"
        })
        .await;
    assert_eq!(*field(&entered, "from"), alpha);
    assert_eq!(*field(&entered, "to"), beta);
    assert_eq!(field(&entered, "app"), "regedit.exe");
    assert_eq!(field(&entered, "app_admin"), true);
    w.check_catalog("Alpha");
    w.check_catalog("Beta");
}

// ---- Story 2 scenario 3 --------------------------------------------------------------------

#[when("Mouser hides or shows the pointer on this computer")]
async fn hides_or_shows_the_pointer(w: &mut LogWorld) {
    let alpha = w.platform("Alpha");
    w.cross_to_beta().await;
    w.back_to_alpha().await;
    w.quickjump(KEY_B);
    until("QuickJump to take the pointer", || alpha.is_grabbed()).await;
    alpha.lose_capture(CaptureLossReason::HooksSilent);
    until("the pointer to be shown", || !alpha.pointer_hidden()).await;
}

#[then("a record says which, and why")]
async fn a_record_says_which_and_why(w: &mut LogWorld) {
    for reason in ["crossing_out", "quickjump"] {
        w.wait_for("Alpha", "pointer.hidden", reason, |r| {
            *field(r, "reason") == reason
        })
        .await;
    }
    for reason in ["returned", "capture_lost"] {
        w.wait_for("Alpha", "pointer.shown", reason, |r| {
            *field(r, "reason") == reason
        })
        .await;
    }
    w.check_catalog("Alpha");
}

// ---- Story 2 scenario 4 --------------------------------------------------------------------

#[when("the operating system refuses an input Mouser passes on")]
async fn the_system_refuses(w: &mut LogWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    beta.set_foreground_app(Some(app("regedit.exe", true)));
    w.cross_to_beta().await;
    let kinds = [
        InputKind::Key,
        InputKind::Button,
        InputKind::Motion,
        InputKind::Scroll,
    ];
    beta.refuse_input(&kinds);
    let send = |down: bool| {
        alpha.send_input(LocalInput::Key { usage: C, down });
        alpha.send_input(LocalInput::Button { button: 1, down });
        alpha.send_input(LocalInput::Scroll { dx: 0.0, dy: -3.0 });
        alpha.send_input(LocalInput::Motion {
            dx: 5.0,
            dy: 0.0,
            abs: (960.0, 540.0),
        });
    };
    send(true);
    for kind in ["key", "button", "motion", "scroll"] {
        w.wait_for("Beta", "input.refused", kind, |r| *field(r, "kind") == kind)
            .await;
    }
    // The system accepts input again: the runs end.
    beta.accept_input();
    send(false);
}

#[then(
    "a record states the kind of input (key, mouse button, pointer move, scroll), the application in front and whether it runs as administrator"
)]
async fn refusal_records(w: &mut LogWorld) {
    let alpha = w.id("Alpha");
    for kind in ["key", "button", "motion", "scroll"] {
        let refused = w
            .wait_for("Beta", "input.refused", kind, |r| *field(r, "kind") == kind)
            .await;
        assert_eq!(refused["level"], "warn");
        assert_eq!(refused["peer"], alpha.as_str());
        assert_eq!(field(&refused, "app"), "regedit.exe");
        assert_eq!(field(&refused, "app_admin"), true);
        let ended = w
            .wait_for("Beta", "input.refused_run_ended", kind, |r| {
                *field(r, "kind") == kind
            })
            .await;
        assert!(field(&ended, "count").as_u64().unwrap() >= 1, "{kind}");
    }
    w.check_catalog("Beta");
}

// ---- Story 2 scenario 5 --------------------------------------------------------------------

#[when("a desk link is lost")]
async fn a_desk_link_is_lost(w: &mut LogWorld) {
    for name in ["Alpha", "Beta", "Gamma"] {
        w.id(name);
    }
    w.lab.node("Beta").stop().await;
    w.wait_for("Alpha", "peer.disconnected", "Beta quit", |r| {
        field(r, "reason") == "peer_quit"
    })
    .await;
    // The network drops everything to and from Gamma.
    w.lab.node("Gamma").sim.set_blackhole(true);
    w.wait_for("Alpha", "peer.disconnected", "Gamma unreachable", |r| {
        field(r, "reason") == "unreachable"
    })
    .await;
    w.lab.node("Gamma").sim.set_blackhole(false);
    w.lab
        .node("Alpha")
        .wait_status("Gamma connected again", |s| {
            s.computers.iter().any(|c| {
                c.name == "Gamma"
                    && c.connection == mouser_proto::control::ConnectionState::Connected
            })
        })
        .await;
    // Mouser quits on this computer.
    w.lab.node("Alpha").stop().await;
}

#[then(
    "a record names the other computer and the reason (the other computer left or quit, became unreachable, runs an incompatible version, a fault on this computer, or this computer quit)"
)]
async fn link_loss_records(w: &mut LogWorld) {
    let (beta, gamma) = (w.id("Beta"), w.id("Gamma"));
    let at_alpha = w.records("Alpha");
    let reason = |peer: &str, reason: &str| {
        at_alpha.iter().any(|r| {
            r["event"] == "peer.disconnected" && r["peer"] == peer && field(r, "reason") == reason
        })
    };
    assert!(reason(&beta, "peer_quit"));
    assert!(reason(&gamma, "unreachable"));
    assert!(reason(&gamma, "quit_here"), "{at_alpha:?}");
    w.check_catalog("Alpha");
}

// ---- Story 2 scenario 6 --------------------------------------------------------------------

#[when("a fault occurs inside Mouser")]
async fn a_fault_occurs(w: &mut LogWorld) {
    let (alpha, beta) = (w.platform("Alpha"), w.platform("Beta"));
    mouser_core::faults::install(w.lab.node("Beta").log());
    w.cross_to_beta().await;
    beta.panic_on_next_inject();
    tap(&alpha, A);
    w.wait_for("Beta", "fault.recovered", "recovery", |_| true)
        .await;
    w.linked().await;
}

#[then(
    "a record describes the fault and where it happened, and a later record shows Mouser carrying on"
)]
async fn fault_records(w: &mut LogWorld) {
    let beta = w.records("Beta");
    let at = |event: &str| beta.iter().position(|r| r["event"] == event);
    let fault = at("fault").expect("a fault record");
    let recovered = at("fault.recovered").expect("a recovery record");
    assert!(fault < recovered);
    let record = &beta[fault];
    assert_eq!(record["level"], "error");
    assert_eq!(field(record, "where"), "read_loop");
    assert!(field(record, "location").as_str().unwrap().contains(".rs:"));
    assert!(!field(record, "message").as_str().unwrap().is_empty());
    assert_eq!(field(&beta[recovered], "where"), "read_loop");
    assert_eq!(field(&beta[recovered], "action"), "closed_link");
    // Mouser carried on: the link came back.
    let connected_after = beta
        .iter()
        .enumerate()
        .any(|(i, r)| i > recovered && r["event"] == "peer.connected");
    assert!(connected_after, "{beta:?}");
    assert!(
        beta.iter()
            .any(|r| { r["event"] == "peer.disconnected" && field(r, "reason") == "fault_here" })
    );
    w.check_catalog("Beta");
}

// ---- Story 2 scenario 7 --------------------------------------------------------------------

#[when(
    "a display is added, removed or rearranged on a computer, or the desk owner moves a computer or display in the arranger"
)]
async fn displays_and_arranger(w: &mut LogWorld) {
    // The owner moves Beta, as a whole, below Alpha.
    w.lab.node("Alpha").wait_layout(3).await;
    let mut layout: Value = w.lab.node("Alpha").call(Method::LayoutGet, json!({})).await;
    let alpha_id = w.id("Alpha");
    let beta_id = w.id("Beta");
    let corner = |layout: &Value, computer: &str, axis: &str| {
        layout["placements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["display_id"]["computer"] == computer)
            .map(|p| p[axis].as_i64().unwrap())
            .min()
            .unwrap()
    };
    let dx = corner(&layout, &alpha_id, "x") - corner(&layout, &beta_id, "x");
    let dy = corner(&layout, &alpha_id, "y") + 296 - corner(&layout, &beta_id, "y");
    for p in layout["placements"].as_array_mut().unwrap() {
        // Gamma, which sits beside Beta, goes along.
        if p["display_id"]["computer"] != alpha_id.as_str() {
            p["x"] = json!(p["x"].as_i64().unwrap() + dx);
            p["y"] = json!(p["y"].as_i64().unwrap() + dy);
        }
    }
    let _: Value = w.lab.node("Alpha").call(Method::LayoutSet, layout).await;
    let beta = w.platform("Beta");
    let original = mouser_platform::Platform::displays(&beta).unwrap();
    let with = |side: LocalDisplay| {
        let mut all = original.clone();
        all.push(side);
        all
    };
    let side = |origin: (f64, f64), logical: (f64, f64)| LocalDisplay {
        key: "side".into(),
        name: "Side".into(),
        pixel_size: (1920, 1080),
        logical_size: logical,
        scale: 1.0,
        physical_size_mm: Some((527.0, 296.0)),
        origin,
    };
    for (displays, change) in [
        (with(side((1920.0, 0.0), (1920.0, 1080.0))), "added"),
        (with(side((0.0, 1080.0), (1920.0, 1080.0))), "rearranged"),
        (with(side((0.0, 1080.0), (1280.0, 720.0))), "resized"),
    ] {
        beta.set_displays(displays);
        w.wait_for("Alpha", "display.changed", change, |r| {
            *field(r, "change") == change
        })
        .await;
    }
    // The owner takes the display out of the layout on Beta and puts it back.
    let full: Value = w.lab.node("Beta").call(Method::LayoutGet, json!({})).await;
    let mut without = full.clone();
    without["placements"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["display_id"]["key"] != "side");
    without["displays"] =
        json!([{"display_id": {"computer": beta_id, "key": "side"}, "included": false}]);
    let _: Value = w.lab.node("Beta").call(Method::LayoutSet, without).await;
    let mut back = full.clone();
    back["displays"] =
        json!([{"display_id": {"computer": beta_id, "key": "side"}, "included": true}]);
    let _: Value = w.lab.node("Beta").call(Method::LayoutSet, back).await;
    // The display goes away.
    beta.set_displays(original);
    w.wait_for("Alpha", "display.changed", "removed", |r| {
        *field(r, "change") == "removed"
    })
    .await;
}

#[then("a record states which computer and display changed and how")]
async fn display_and_layout_records(w: &mut LogWorld) {
    let beta = w.id("Beta");
    let alpha = w.id("Alpha");
    for name in ["Alpha", "Beta"] {
        for change in ["added", "rearranged", "resized", "removed"] {
            let record = w
                .wait_for(name, "display.changed", change, |r| {
                    *field(r, "change") == change
                })
                .await;
            assert_eq!(*field(&record, "computer"), beta);
            assert_eq!(field(&record, "display"), "Side");
        }
    }
    let at_beta = w.records("Beta");
    for change in ["display_removed", "display_included"] {
        assert!(
            at_beta
                .iter()
                .any(|r| r["event"] == "layout.changed" && *field(r, "change") == change),
            "{change}"
        );
    }
    let moved = w
        .wait_for("Alpha", "layout.changed", "moved", |r| {
            *field(r, "change") == "moved"
        })
        .await;
    assert_eq!(*field(&moved, "by"), alpha);
    assert_eq!(*field(&moved, "computer"), beta);
    // Beta was placed by the desk when it joined.
    w.wait_for("Alpha", "layout.changed", "auto_placed", |r| {
        *field(r, "change") == "auto_placed" && *field(r, "computer") == beta
    })
    .await;
    w.check_catalog("Alpha");
    w.check_catalog("Beta");
}

// ---- Story 2 scenario 8 --------------------------------------------------------------------

#[when("a setting changes")]
async fn a_setting_changes(w: &mut LogWorld) {
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(
            Method::ConfigSet,
            json!({"key": "mouse.buttons_4_5", "value": "back_forward"}),
        )
        .await;
    let _: Empty = w
        .lab
        .node("Alpha")
        .call(
            Method::ConfigSet,
            json!({"key": "general.notifications", "value": false, "origin": "app"}),
        )
        .await;
    // The configuration file, edited with a text editor.
    let path = w.lab.node("Alpha").dir.path().join("config.toml");
    let mut table: toml::Table = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    table["general"]["start_at_login"] = false.into();
    std::fs::write(&path, toml::to_string_pretty(&table).unwrap()).unwrap();
    w.wait_for("Alpha", "setting.changed", "the edit", |r| {
        *field(r, "key") == "general.start_at_login"
    })
    .await;
    // Another desk computer.
    let _: Empty = w
        .lab
        .node("Beta")
        .call(
            Method::ConfigSet,
            json!({"key": "crossing.block_fullscreen", "value": false}),
        )
        .await;
}

#[then(
    "a record names the setting, its old and new values, and where it was changed (app window, command-line tool, configuration file, or another desk computer)"
)]
async fn setting_records(w: &mut LogWorld) {
    let beta = w.id("Beta");
    for (key, old, new, origin) in [
        (
            "mouse.buttons_4_5",
            json!("passthrough"),
            json!("back_forward"),
            "cli",
        ),
        ("general.notifications", json!(true), json!(false), "app"),
        ("general.start_at_login", json!(true), json!(false), "file"),
        (
            "crossing.block_fullscreen",
            json!(true),
            json!(false),
            "desk_mate",
        ),
    ] {
        let record = w
            .wait_for("Alpha", "setting.changed", key, |r| *field(r, "key") == key)
            .await;
        assert_eq!(*field(&record, "old"), old, "{key}");
        assert_eq!(*field(&record, "new"), new, "{key}");
        assert_eq!(field(&record, "origin"), origin, "{key}");
        if origin == "desk_mate" {
            assert_eq!(*field(&record, "from"), beta);
        }
    }
    // The same setting at the computer it was made on.
    let record = w
        .wait_for("Beta", "setting.changed", "block_fullscreen", |r| {
            *field(r, "key") == "crossing.block_fullscreen"
        })
        .await;
    assert_eq!(field(&record, "origin"), "cli");
    w.check_catalog("Alpha");
    w.check_catalog("Beta");
}

// ---- Story 2 scenario 9 --------------------------------------------------------------------

#[when("clipboard content moves between computers")]
async fn clipboard_moves(w: &mut LogWorld) {
    let alpha = w.platform("Alpha");
    alpha.set_local_clipboard(vec![
        mouser_platform::ClipRep::Text("hello there".into()),
        mouser_platform::ClipRep::Html("<p>hello</p>".into()),
    ]);
    let beta = w.platform("Beta");
    until("Beta to hold the clipboard", || {
        !beta.clipboard_contents().is_empty()
    })
    .await;
}

#[then("a record names the two computers and each representation's type and size")]
async fn clipboard_records(w: &mut LogWorld) {
    let (alpha, beta) = (w.id("Alpha"), w.id("Beta"));
    let expected = json!({
        "from": alpha, "to": beta,
        "reps": [{"kind": "text", "size": 11}, {"kind": "html", "size": 12}]
    });
    let sent = w
        .wait_for("Alpha", "clipboard.sent", "sent to Beta", |r| {
            r["peer"] == beta.as_str()
        })
        .await;
    assert_eq!(sent["fields"], expected);
    assert_eq!(sent["peer"], beta.as_str());
    let received = w
        .wait_for("Beta", "clipboard.received", "received", |_| true)
        .await;
    assert_eq!(received["fields"], expected);
    assert_eq!(received["peer"], alpha.as_str());
    w.check_catalog("Alpha");
    w.check_catalog("Beta");
}

// ---- Story 2 scenario 10 and the edge cases -------------------------------------------------

#[given("a desk with several computers")]
async fn a_desk_with_several_computers(w: &mut LogWorld) {
    let setups = vec![
        ("Alpha", setup("0.3.0", false)),
        ("Beta", setup("0.3.0", false)),
        ("Gamma", setup("0.2.8", false)),
    ];
    w.lab
        .form_desk("Home office", "Alpha", &["Beta", "Gamma"], setups)
        .await;
    for name in ["Alpha", "Beta", "Gamma"] {
        w.logging_on(name).await;
    }
}

#[when("the agent asks the command-line tool to collect the desk's logs")]
async fn the_agent_collects(w: &mut LogWorld) {
    w.collect().await;
}

#[then("it produces one bundle file on this computer whose contents follow FR-007")]
async fn one_bundle_file(w: &mut LogWorld) {
    let out = w.out.as_ref().unwrap().path().to_path_buf();
    let written: Vec<_> = std::fs::read_dir(&out).unwrap().collect();
    assert_eq!(written.len(), 1, "one file");
    let mut zip = w.zip();
    let manifest = zip.manifest();
    assert!(logs::bundle_validator().is_valid(&manifest), "{manifest}");
    let computers = manifest["computers"].as_array().unwrap();
    assert_eq!(computers.len(), 3);
    assert_eq!(
        computers[0]["id"],
        w.id("Alpha").as_str(),
        "requester first"
    );
    let names = zip.names();
    // The two computers that answered are in, with their logs and settings.
    for (folder, included) in [("Alpha", true), ("Beta", true), ("Gamma", false)] {
        let entry = computers
            .iter()
            .find(|c| c["name"] == folder)
            .unwrap_or_else(|| panic!("{folder} is listed"));
        assert_eq!(entry["included"], included, "{folder}");
        let prefix = entry["folder"].as_str().unwrap();
        assert!(names.contains(&format!("{prefix}/computer.json")));
        assert_eq!(
            names.contains(&format!("{prefix}/mouser.log")),
            included,
            "{folder}"
        );
    }
    // The log of the other computer is the log of the other computer.
    let beta = computers.iter().find(|c| c["name"] == "Beta").unwrap();
    let log = zip.text(&format!("{}/mouser.log", beta["folder"].as_str().unwrap()));
    let first: Value = serde_json::from_str(log.lines().next().unwrap()).unwrap();
    assert_eq!(first["event"], "log.header");
    assert_eq!(field(&first, "computer_name"), "Beta");
    // What FR-007 forbids is nowhere in it.
    for name in &names {
        for banned in ["state.json", "secrets", "device", "desk_key"] {
            assert!(!name.contains(banned), "{name}");
        }
    }
    // The command-line tool says who is in it.
    let listed = &w.bundle.as_ref().unwrap().computers;
    assert_eq!(listed.len(), 3);
    assert_eq!(listed[2].reason, Some(BundleReason::OlderVersion));
}

#[given("the diagnostic log rotates in the middle of an incident")]
async fn the_log_rotates(w: &mut LogWorld) {
    w.desk(&["Alpha", "Beta"]).await;
    w.rotate("Alpha").await;
}

#[when("the agent opens the new file and collects the desk's logs")]
async fn opens_the_new_file_and_collects(w: &mut LogWorld) {
    let first = w.first_record("Alpha");
    w.opened.push(first);
    w.collect().await;
}

#[then(
    "the new file starts with the record of Story 2 scenario 1, and the bundle includes both files"
)]
async fn new_file_and_both_files(w: &mut LogWorld) {
    let header = &w.opened[0];
    assert_eq!(header["event"], "log.header");
    assert_eq!(field(header, "computer_name"), "Alpha");
    assert_eq!(field(header, "desk_name"), "Home office");
    let mut zip = w.zip();
    let manifest = zip.manifest();
    let alpha = manifest["computers"][0].clone();
    assert_eq!(alpha["files"], json!(["mouser.log", "mouser.log.1"]));
    let folder = alpha["folder"].as_str().unwrap();
    let current = zip.text(&format!("{folder}/mouser.log"));
    let older = zip.text(&format!("{folder}/mouser.log.1"));
    for (file, text) in [("mouser.log", current), ("mouser.log.1", older)] {
        let first: Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
        assert_eq!(first["event"], "log.header", "{file}");
    }
}

#[given("a desk computer is offline, or has logging off, when logs are collected")]
async fn offline_or_logging_off(w: &mut LogWorld) {
    let setups = vec![
        ("Alpha", setup("0.3.0", true)),
        ("Beta", setup("0.3.0", true)),
        ("Gamma", setup("0.3.0", false)),
    ];
    w.lab
        .form_desk("Home office", "Alpha", &["Beta", "Gamma"], setups)
        .await;
    w.lab.node("Beta").stop().await;
    w.lab
        .node("Alpha")
        .wait_status("Beta to be gone", |s| {
            s.computers.iter().any(|c| {
                c.name == "Beta"
                    && c.connection != mouser_proto::control::ConnectionState::Connected
            })
        })
        .await;
}

#[then("the bundle lists that computer with that reason")]
async fn the_bundle_lists_the_reason(w: &mut LogWorld) {
    let listed = w.bundle.clone().unwrap().computers;
    let reason = |name: &str| listed.iter().find(|c| c.name == name).unwrap().reason;
    assert_eq!(reason("Beta"), Some(BundleReason::Offline));
    assert_eq!(reason("Gamma"), Some(BundleReason::LoggingOff));
    assert_eq!(reason("Alpha"), None);
    let mut zip = w.zip();
    let manifest = zip.manifest();
    assert!(logs::bundle_validator().is_valid(&manifest), "{manifest}");
    for entry in manifest["computers"].as_array().unwrap() {
        if entry["included"] == false {
            assert!(entry["reason"].is_string(), "{entry}");
        }
    }
    // The computer with logging off still has its card and settings.
    let gamma = manifest["computers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "Gamma")
        .unwrap();
    assert_eq!(gamma["settings_file"], "config.toml");
    let folder = gamma["folder"].as_str().unwrap();
    assert!(
        zip.text(&format!("{folder}/config.toml"))
            .contains("[diagnostics]")
    );
}
