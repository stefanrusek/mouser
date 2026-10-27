//! Contract test (producer side) for contracts/log-events.md: a scripted
//! session of two computers on a desk triggers every event core emits, and
//! every line of every log file validates against the closed catalog
//! (`log-events.schema.json`, with `log-record.schema.json` registered as the
//! resource it refers to).

mod steps;
mod support;

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use mouser_core::identity::Identity;
use mouser_core::session::handshake::exchange;
use mouser_core::transport::{Link, Transport};
use mouser_platform::{ClipRep, ForegroundApp, LocalDisplay, LocalInput, Platform as _};
use mouser_proto::control::{Empty, LoggingSetResult, LogsCollectResult, Method};
use mouser_proto::wire::ControlMessage;
use mouser_testkit::{Injected, InputKind};
use serde::Serialize;
use serde_json::{Value, json};
use support::desk::{Lab, Node, Setup};
use support::logs;
use support::release::{Host, Signer};

/// Every event of the catalog that core emits (the others are written by the
/// platform backends, the app and the service).
const CORE_EMITS: [&str; 25] = [
    "log.header",
    "logging.enabled",
    "logging.disabled",
    "control.changed",
    "pointer.hidden",
    "pointer.shown",
    "input.refused",
    "input.refused_run_ended",
    "peer.connected",
    "peer.disconnected",
    "wire.unknown",
    "fault",
    "fault.recovered",
    "display.changed",
    "layout.changed",
    "setting.changed",
    "clipboard.sent",
    "clipboard.received",
    "logs.collected",
    "problem.raised",
    "problem.cleared",
    "config.rejected",
    "update.available",
    "update.failed",
    "update.check_failed",
];

/// A message from the future: a variant of the control message that no
/// build of the protocol knows yet.
struct FromTheFuture(u32);

impl Serialize for FromTheFuture {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_unit_variant("ControlMessage", self.0, "Later")
    }
}

const WAIT: Duration = Duration::from_secs(20);

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

async fn record(node: &Node, event: &str) {
    until(&format!("{} to log {event}", node.name), || {
        logs::records(node).iter().any(|r| r["event"] == event)
    })
    .await;
}

fn tap(node: &Node, usage: u32) {
    for down in [true, false] {
        node.platform.send_input(LocalInput::Key { usage, down });
    }
}

#[tokio::test]
async fn a_scripted_session_writes_every_event_core_emits_and_only_the_catalog() {
    let mut lab = Lab::new();
    let signer = Signer::new();
    let host = Host::start().await;
    host.publish(&signer, "9.9.9");
    let mut alpha = Setup {
        mouser_version: "0.3.0".into(),
        release_pubkey: Some(signer.pubkey()),
        ..Setup::default()
    };
    alpha.config.diagnostics.max_log_mb = 1;
    alpha.config.updates.release_url = host.release_url();
    let mut beta = Setup {
        mouser_version: "0.3.0".into(),
        release_pubkey: Some(signer.pubkey()),
        ..Setup::default()
    };
    beta.config.updates.release_url = host.release_url();
    lab.form_desk(
        "Home office",
        "Alpha",
        &["Beta"],
        vec![("Alpha", alpha), ("Beta", beta)],
    )
    .await;
    // A computer of its own, looking for releases where there are none.
    let mut delta = Setup {
        mouser_version: "0.3.0".into(),
        check_schedule: Some(mouser_core::release::CheckSchedule {
            first: Duration::from_millis(20),
            every: Duration::from_secs(3600),
        }),
        ..Setup::default()
    };
    delta.config.diagnostics.logging = true;
    delta.config.updates.release_url = "http://127.0.0.1:1/latest.json".into();
    lab.add_with("Delta", delta).await;

    // Logging is switched on, off and on again.
    for name in ["Alpha", "Beta"] {
        let _: LoggingSetResult = lab
            .node(name)
            .call(Method::LoggingSet, json!({"enabled": true}))
            .await;
    }
    let _: LoggingSetResult = lab
        .node("Beta")
        .call(Method::LoggingSet, json!({"enabled": false}))
        .await;
    let _: LoggingSetResult = lab
        .node("Beta")
        .call(Method::LoggingSet, json!({"enabled": true}))
        .await;

    // Control moves, the pointer hides and shows, input is refused and
    // accepted again.
    lab.node("Beta")
        .platform
        .set_foreground_app(Some(ForegroundApp {
            name: "regedit.exe".into(),
            admin: true,
            outranks_mouser: false,
        }));
    let (a, b) = (
        lab.node("Alpha").platform.clone(),
        lab.node("Beta").platform.clone(),
    );
    let push = |dx: f64, abs: (f64, f64)| {
        a.send_input(LocalInput::Motion { dx, dy: 0.0, abs });
    };
    push(1.0, (1919.0, 540.0));
    until("Beta to have the pointer", || {
        a.is_grabbed()
            && b.injected()
                .iter()
                .any(|i| matches!(i, Injected::MotionAbs { .. }))
    })
    .await;
    b.refuse_input(&[InputKind::Key]);
    a.send_input(LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    });
    record(lab.node("Beta"), "input.refused").await;
    b.accept_input();
    a.send_input(LocalInput::Key {
        usage: 0x0007_0004,
        down: false,
    });
    record(lab.node("Beta"), "input.refused_run_ended").await;
    push(-1.0, (960.0, 540.0));
    until("the pointer to come back", || !a.pointer_hidden()).await;

    // A display, the arranger, a setting.
    let original = b.displays().unwrap();
    let mut with_side = original.clone();
    with_side.push(LocalDisplay {
        key: "side".into(),
        name: "Side".into(),
        pixel_size: (1920, 1080),
        logical_size: (1920.0, 1080.0),
        scale: 1.0,
        physical_size_mm: Some((527.0, 296.0)),
        origin: (1920.0, 0.0),
    });
    b.set_displays(with_side);
    record(lab.node("Alpha"), "display.changed").await;
    let mut layout: Value = lab.node("Alpha").call(Method::LayoutGet, json!({})).await;
    let alpha_id = lab.node("Alpha").id().to_string();
    let at = layout["placements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["display_id"]["computer"] == alpha_id.as_str())
        .unwrap()
        .clone();
    // Beta, as a whole, goes below Alpha.
    let beta_id = lab.node("Beta").id().to_string();
    let corner = |layout: &Value, axis: &str| {
        layout["placements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["display_id"]["computer"] == beta_id.as_str())
            .map(|p| p[axis].as_i64().unwrap())
            .min()
            .unwrap()
    };
    let (dx, dy) = (
        at["x"].as_i64().unwrap() - corner(&layout, "x"),
        at["y"].as_i64().unwrap() + 296 - corner(&layout, "y"),
    );
    for p in layout["placements"].as_array_mut().unwrap() {
        if p["display_id"]["computer"] == beta_id.as_str() {
            p["x"] = json!(p["x"].as_i64().unwrap() + dx);
            p["y"] = json!(p["y"].as_i64().unwrap() + dy);
        }
    }
    let _: Value = lab.node("Alpha").call(Method::LayoutSet, layout).await;
    let _: Empty = lab
        .node("Alpha")
        .call(
            Method::ConfigSet,
            json!({"key": "crossing.block_fullscreen", "value": false}),
        )
        .await;

    // The clipboard.
    a.set_local_clipboard(vec![ClipRep::Text("hello".into())]);
    record(lab.node("Beta"), "clipboard.received").await;

    // A problem that comes and goes.
    b.set_secure_input(Some(mouser_platform::SecureInputHolder {
        pid: 1,
        app_name: "Terminal".into(),
    }));
    record(lab.node("Alpha"), "problem.raised").await;
    b.set_secure_input(None);
    record(lab.node("Alpha"), "problem.cleared").await;

    // A settings file with a mistake.
    let config = lab.node("Alpha").dir.path().join("config.toml");
    let good = std::fs::read_to_string(&config).unwrap();
    std::fs::write(&config, good.replace("port = ", "port = 0 #")).unwrap();
    record(lab.node("Alpha"), "config.rejected").await;
    std::fs::write(&config, good).unwrap();

    // Releases.
    // Whichever of the two is elected looks, and tells the other.
    for name in ["Alpha", "Beta"] {
        let _: Value = lab.node(name).call(Method::UpdateCheck, json!({})).await;
    }
    record(lab.node("Alpha"), "update.available").await;
    record(lab.node("Delta"), "update.check_failed").await;

    // A message from the future, skipped.
    let identity = Identity::from_seed([0x55; 32]);
    let transport = Transport::bind(&identity, 0);
    let beta_port = mouser_core::config::load_or_create(lab.node("Beta").dir.path())
        .advanced
        .port;
    let beta_addr = SocketAddr::from(([127, 0, 0, 1], u16::try_from(beta_port).unwrap()));
    let conn = transport
        .connect(beta_addr, Some(lab.node("Beta").id()))
        .await
        .unwrap();
    let mut link = Link::open(conn).await.unwrap();
    let hello = ControlMessage::Hello {
        protocol_version: mouser_proto::wire::PROTOCOL_VERSION,
        mouser_version: "0.3.0".into(),
        computer_id: identity.computer_id(),
        display_name: "Rogue".into(),
        os: mouser_proto::model::Os::Linux,
        desk_id: None,
    };
    exchange(&mut link, hello).await.unwrap();
    link.writer.send(&FromTheFuture(60)).await.unwrap();
    record(lab.node("Beta"), "wire.unknown").await;

    // A fault on Beta, which closes the link and heals. Beta is below
    // Alpha now.
    mouser_core::faults::install(lab.node("Beta").log());
    a.send_input(LocalInput::Motion {
        dx: 0.0,
        dy: 1.0,
        abs: (960.0, 1079.0),
    });
    until("Beta to have the pointer again", || a.is_grabbed()).await;
    b.panic_on_next_inject();
    tap(lab.node("Alpha"), 0x0007_0004);
    record(lab.node("Beta"), "fault.recovered").await;

    // The link heals.
    record(lab.node("Beta"), "peer.connected").await;
    lab.node("Alpha")
        .wait_status("both computers to be connected", |s| {
            s.computers
                .iter()
                .any(|c| c.connection == mouser_proto::control::ConnectionState::Connected)
        })
        .await;

    // Everyone's logs, in one bundle.
    let out = tempfile::tempdir().unwrap();
    let _: LogsCollectResult = lab
        .node("Alpha")
        .call(
            Method::LogsCollect,
            json!({"dir": out.path().to_str().unwrap()}),
        )
        .await;
    record(lab.node("Alpha"), "logs.collected").await;

    // Every line of every file is in the catalog, and every event core
    // emits has been seen.
    let mut seen = BTreeSet::new();
    for name in ["Alpha", "Beta", "Delta"] {
        logs::assert_every_record_is_in_the_catalog(lab.node(name));
        for r in logs::records(lab.node(name)) {
            seen.insert(r["event"].as_str().unwrap().to_string());
        }
    }
    let missing: Vec<_> = CORE_EMITS
        .iter()
        .filter(|event| !seen.contains(**event))
        .collect();
    assert!(
        missing.is_empty(),
        "never written: {missing:?}; saw {seen:?}"
    );
    lab.shutdown().await;
}
