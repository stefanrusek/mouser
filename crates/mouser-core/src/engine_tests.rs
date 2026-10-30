use std::sync::Arc;

use mouser_proto::control::{Empty, Method};
use mouser_testkit::VirtualPlatform;
use serde_json::json;

use crate::EngineEvent;
use crate::crossing::state::Control;
use crate::lock::LockExt;
use crate::testing::*;
use crate::{Engine, EngineParams};

#[tokio::test]
async fn start_serves_the_control_api_in_process_and_shutdown_removes_the_socket() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let path = node.control_path.clone();
    if cfg!(unix) {
        assert!(path.exists(), "the socket is a file on Unix");
    }
    assert_eq!(status(&node).await.computer.name, "Alpha");
    let handle = node.engine.control();
    let mut notes = handle.notifications();
    let _: Empty = call(&node, Method::ActivePause, json!({})).await;
    assert_eq!(
        notes.recv().await.unwrap().method,
        mouser_proto::control::Topic::Status
    );
    let err = call_err(&node, Method::Subscribe, json!({})).await;
    assert_eq!(err.code, -32601);
    node.engine.shutdown().await;
    assert!(!path.exists());
}

/// FR/US1: shutdown ends the control endpoint. By the time it returns no
/// client gets in, on Windows too, where the endpoint is a pipe (not a file).
#[tokio::test]
async fn once_shut_down_the_control_endpoint_takes_no_client() {
    use crate::control::client::ControlClient;
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let path = node.control_path.clone();
    drop(
        ControlClient::connect(&path)
            .await
            .expect("a client gets in while it runs"),
    );
    node.engine.shutdown().await;
    assert!(ControlClient::connect(&path).await.is_err());
}

#[tokio::test]
async fn a_configured_computer_name_replaces_the_operating_system_name() {
    let lab = Lab::new();
    let node = lab
        .start_with("os-name", VirtualPlatform::new(), |c| {
            c.general.computer_name = "Studio".into()
        })
        .await;
    assert_eq!(status(&node).await.computer.name, "Studio");
}

#[tokio::test]
async fn without_a_discovery_backend_the_engine_uses_real_mdns() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = mouser_proto::config::Config::default();
    config.advanced.port = u32::from(crate::testing::free_port());
    crate::config::save(dir.path(), &config);
    let params = EngineParams::new(
        Arc::new(VirtualPlatform::new()),
        dir.path().to_path_buf(),
        dir.path().join("mouser.sock"),
        "Alpha".into(),
        "0.1.0".into(),
    );
    let engine = Engine::start(params).await;
    assert_eq!(
        engine.local_addr().port(),
        u16::try_from(config.advanced.port).unwrap()
    );
    assert_eq!(
        engine.log().path(),
        dir.path().join("logs").join(crate::logging::FILE)
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn events_without_a_control_topic_are_available_in_process() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let mut events = node.engine.control().events();
    assert_eq!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    );
    assert!(node.engine.layout().placements.is_empty());
    assert!(node.engine.needs_review().is_empty());
}

/// The next `Fault` event within 10 s.
async fn fault_at(events: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> String {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            if let EngineEvent::Fault { r#where } = events.recv().await.unwrap() {
                return r#where;
            }
        }
    })
    .await
    .expect("the fault is reported")
}

#[tokio::test]
async fn a_panic_in_a_reconcile_tick_recovers_and_the_next_tick_still_runs() {
    use mouser_testkit::Injected;
    use std::sync::atomic::Ordering;
    let lab = Lab::new();
    let (alpha, knobs) = lab.start_flaky("Alpha").await;
    let nodes = desk_from(vec![alpha, lab.start("Beta").await]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    assert!(a.platform.pointer_hidden());
    let mut events = a.engine.events();

    knobs.panic_secure_input.store(true, Ordering::SeqCst);

    assert_eq!(fault_at(&mut events).await, "reconcile");
    // Recovered: control is back, the pointer is shown, Beta let go.
    eventually("control to come back", || async {
        a.engine.ctx.crossing.guard().session.control == Control::Local
    })
    .await;
    assert!(!a.platform.pointer_hidden());
    eventually("Beta to stop receiving", || async {
        b.engine.ctx.crossing.guard().receiving.is_none()
    })
    .await;
    // The loop carried on: pause, the panic key and crossing all work.
    assert_eq!(connected(a).await, 1);
    push_right(a);
    eventually("crossing to work again", || async {
        a.engine.ctx.crossing.guard().session.remote().is_some()
    })
    .await;
    let _: Empty = call(a, Method::ActivePause, json!({})).await;
    eventually("pause to bring control back", || async {
        a.engine.ctx.crossing.guard().session.control == Control::Local
    })
    .await;
    assert!(!a.platform.pointer_hidden());
}

#[tokio::test]
async fn persist_returns_an_error_when_state_json_is_not_writable_and_the_state_is_kept() {
    use mouser_proto::control::Desk;
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let path = node.engine.ctx.state_path();
    // A directory where the file belongs cannot be written to.
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    {
        let core = node.engine.ctx.core.guard();
        assert!(node.engine.ctx.persist(&core).is_err());
    }
    // Nothing panics and the in-memory state is kept: the desk exists.
    let desk: Desk = call(&node, Method::DeskCreate, json!({"name": "Home office"})).await;
    {
        let core = node.engine.ctx.core.guard();
        assert_eq!(core.state.desk.as_ref().unwrap().desk_id, desk.desk_id);
    }
    assert_eq!(status(&node).await.computer.name, "Alpha");
}

#[tokio::test]
async fn shutdown_while_remote_returns_locally_releases_and_closes_every_link() {
    use mouser_platform::LocalInput;
    use mouser_testkit::Injected;
    const A: u32 = 0x0007_0004;
    let lab = Lab::new();
    let (alpha, knobs) = lab.start_flaky("Alpha").await;
    let mut nodes = desk_from(vec![alpha, lab.start("Beta").await]).await;
    let beta = nodes.pop().unwrap();
    let alpha = nodes.pop().unwrap();
    push_right(&alpha);
    injected_where(&beta.platform, "entry", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    alpha.platform.send_input(LocalInput::Key {
        usage: A,
        down: true,
    });
    injected_where(&beta.platform, "the held key", |i| {
        *i == Injected::Key {
            usage: A,
            down: true,
        }
    })
    .await;
    assert!(alpha.platform.pointer_hidden());

    let (platform, grabs) = (alpha.platform.clone(), knobs.grabs.clone());
    alpha.engine.shutdown().await;

    assert!(!platform.pointer_hidden(), "the pointer is shown again");
    assert_eq!(grabs.guard().last(), Some(&false));
    // Beta lets go of what Alpha held and sees the link close.
    injected_where(&beta.platform, "the release", |i| {
        *i == Injected::Key {
            usage: A,
            down: false,
        }
    })
    .await;
    eventually("Beta to see Alpha leave", || async {
        connected(&beta).await == 0 && beta.engine.ctx.crossing.guard().receiving.is_none()
    })
    .await;
}

#[test]
fn the_engine_mode_is_a_computer_mode_and_a_new_engine_is_installed() {
    use mouser_proto::wire::ComputerMode;
    let params = EngineParams::new(
        Arc::new(VirtualPlatform::new()),
        std::path::PathBuf::from("config"),
        std::path::PathBuf::from("mouser.sock"),
        "Alpha".into(),
        "0.3.0".into(),
    );
    let mode: ComputerMode = params.mode;
    assert_eq!(mode, ComputerMode::Installed);
}

#[tokio::test]
async fn harness_nodes_are_installed_and_status_reports_the_mode_of_this_computer() {
    use mouser_proto::wire::ComputerMode;
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let me = status(&nodes[0]).await;
    let mine = me
        .computers
        .iter()
        .find(|c| c.id == me.computer.id)
        .unwrap();
    assert_eq!(mine.mode, Some(ComputerMode::Installed));
    for mode in [
        ComputerMode::Portable,
        ComputerMode::PortableElevated,
        ComputerMode::Service,
        ComputerMode::Headless,
    ] {
        let node = lab
            .start_released("Mode", |_| {}, move |params| params.mode = mode)
            .await;
        let s = status(&node).await;
        let mine = s.computers.iter().find(|c| c.id == s.computer.id).unwrap();
        assert_eq!(mine.mode, Some(mode));
        node.engine.shutdown().await;
    }
}

#[tokio::test]
async fn a_state_that_cannot_be_saved_is_a_fault_in_the_log_and_the_state_is_kept() {
    let lab = Lab::new();
    let node = lab
        .start_with("Alpha", VirtualPlatform::new(), |c| {
            c.diagnostics.logging = true
        })
        .await;
    let path = node.engine.ctx.state_path();
    std::fs::remove_file(&path).unwrap();
    std::fs::create_dir(&path).unwrap();
    let line = line!() + 1;
    let failed = node.engine.ctx.persist(&node.engine.ctx.core.guard());
    assert!(failed.is_err());
    let text = std::fs::read_to_string(node.engine.log().path()).unwrap();
    let fault: serde_json::Value = text
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|r| r["event"] == "fault")
        .expect("a fault record");
    assert_eq!(fault["level"], "error");
    assert_eq!(fault["fields"]["where"], "persist");
    assert_eq!(fault["fields"]["message"], "state.json could not be saved");
    let location = fault["fields"]["location"].as_str().unwrap();
    assert!(
        location.ends_with(&format!("engine_tests.rs:{line}")),
        "{location}"
    );
}

/// Shutdown returns with the port given back, so an engine restarted on the
/// same port (an update, a headless restart) binds it at once. Rounds repeat
/// because one can win the race by luck.
#[tokio::test(flavor = "multi_thread")]
async fn the_port_is_free_the_moment_shutdown_returns() {
    let lab = Lab::new();
    for _ in 0..20 {
        let node = lab.start("Alpha").await;
        let port = node.engine.local_addr().port();
        node.engine.shutdown().await;
        std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, port))
            .expect("IPv4 port is free");
    }
}

/// A network change that arrives after shutdown has begun does nothing: the
/// engine neither panics nor rebinds the port it gave back.
#[tokio::test]
async fn a_network_change_after_shutdown_is_ignored() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let ctx = node.engine.ctx.clone();
    node.engine.shutdown().await;
    // The physical address moves, so the change would rebind the endpoint.
    node.platform
        .set_interfaces(vec![mouser_platform::NetInterface {
            name: "eth0".into(),
            kind: mouser_platform::InterfaceKind::PhysicalLan,
            addrs: vec![std::net::Ipv4Addr::new(192, 168, 7, 7).into()],
        }]);
    ctx.on_network_change();
}

/// A link adopted once shutdown has stopped the engine's tasks (its
/// handshake finished on another thread as the engine quit) is forgotten
/// all the same: nothing keeps its connection, which would hold the
/// endpoint's socket and keep shutdown from ever giving the port back.
#[tokio::test]
async fn a_link_adopted_after_the_tasks_stopped_is_forgotten() {
    use crate::identity::Identity;
    use crate::transport::{Link, Transport};
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let ctx = node.engine.ctx.clone();
    node.engine.shutdown().await;
    let (ia, ib) = (Identity::from_seed([1; 32]), Identity::from_seed([2; 32]));
    let (a, b) = (Transport::bind(&ia, 0), Transport::bind(&ib, 0));
    let to = std::net::SocketAddr::from(([127, 0, 0, 1], b.local_addr().port()));
    let (dialed, _accepted) = handshake(&a, &b, to, Some(ib.computer_id())).await;
    let link = Link::open(dialed).await.expect("the connection is open");
    let hello = crate::session::handshake::PeerHello {
        computer_id: ib.computer_id(),
        mouser_version: "0.3.0".into(),
        protocol_version: mouser_proto::wire::PROTOCOL_VERSION,
        display_name: "Beta".into(),
        os: mouser_proto::model::Os::Macos,
        desk_id: None,
    };
    ctx.adopt(link, hello, None, true);
    eventually("the late link to be forgotten", || async {
        ctx.core.guard().peers.is_empty()
    })
    .await;
}
