use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use mouser_proto::ids::ComputerId;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::EngineEvent;
use crate::faults::{Faulted, run_unit, run_unit_async};
use crate::lock::LockExt;
use crate::logging::Log;
use crate::testing::*;

fn events() -> (
    broadcast::Sender<EngineEvent>,
    broadcast::Receiver<EngineEvent>,
) {
    broadcast::channel(8)
}

#[test]
fn a_unit_that_does_not_panic_returns_its_value() {
    let (tx, mut rx) = events();
    assert_eq!(run_unit(&tx, "reconcile", || 7), Ok(7));
    assert!(rx.try_recv().is_err());
}

#[test]
fn a_unit_that_panics_is_faulted_reported_and_the_caller_continues() {
    let (tx, mut rx) = events();
    let mut ticks = 0;
    for tick in 0..3 {
        let result = run_unit(&tx, "reconcile", || {
            if tick == 1 {
                panic!("scripted fault");
            }
            tick
        });
        ticks += 1;
        assert_eq!(result.is_err(), tick == 1);
    }
    assert_eq!(ticks, 3);
    assert_eq!(
        rx.try_recv().unwrap(),
        EngineEvent::Fault {
            r#where: "reconcile".into()
        }
    );
    assert!(rx.try_recv().is_err());
    assert_eq!(
        run_unit(&tx, "x", || -> u8 { panic!("again") }),
        Err(Faulted)
    );
}

#[tokio::test]
async fn an_async_unit_returns_its_value_across_awaits() {
    let (tx, mut rx) = events();
    let value = run_unit_async(&tx, "control", async {
        tokio::task::yield_now().await;
        9
    })
    .await;
    assert_eq!(value, Ok(9));
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn an_async_unit_that_panics_is_faulted_and_reported() {
    let (tx, mut rx) = events();
    let value = run_unit_async(&tx, "control", async {
        tokio::task::yield_now().await;
        if true {
            panic!("scripted fault");
        }
        1
    })
    .await;
    assert_eq!(value, Err(Faulted));
    assert_eq!(
        rx.try_recv().unwrap(),
        EngineEvent::Fault {
            r#where: "control".into()
        }
    );
}

const ME: ComputerId = ComputerId([7; 16]);

fn records(log: &Log) -> Vec<Value> {
    read_log(&log.path())
}

/// Installing a hook takes the current one and sets the new one: two tests
/// doing so at once would lose one of them.
static HOOKS: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn install(log: Arc<Log>) {
    let _one_at_a_time = HOOKS.guard();
    crate::faults::install(log);
}

/// A log with logging on, and the panic hook writing to it. The hook that
/// was installed before is counted, so that chaining to it can be seen.
fn hooked(dir: &tempfile::TempDir, enabled: bool) -> (Arc<Log>, Arc<AtomicUsize>) {
    let _one_at_a_time = HOOKS.guard();
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        seen.fetch_add(1, Ordering::SeqCst);
        before(info);
    }));
    let log = Arc::new(Log::new(dir.path(), ME, "Alpha".into(), enabled, 1));
    crate::faults::install(log.clone());
    (log, calls)
}

/// Run `panic` on a thread called `name` and wait for it to die.
fn panics_on(name: &str, panic: fn()) {
    let result = std::thread::Builder::new()
        .name(name.into())
        .spawn(panic)
        .unwrap()
        .join();
    assert!(result.is_err());
}

static LINE: AtomicU32 = AtomicU32::new(0);

fn boom() {
    LINE.store(line!(), Ordering::SeqCst);
    panic!("boom {}", 1)
}

fn fault_with(log: &Log, message: &str) -> Value {
    records(log)
        .into_iter()
        .find(|r| r["event"] == "fault" && r["fields"]["message"] == message)
        .unwrap_or_else(|| panic!("a fault record for {message:?}: {:?}", records(log)))
}

#[test]
fn a_panic_writes_a_fault_with_thread_location_and_message_and_then_the_previous_hook_runs() {
    let dir = tempfile::tempdir().unwrap();
    let (log, calls) = hooked(&dir, true);
    panics_on("worker-7", boom);
    assert!(calls.load(Ordering::SeqCst) >= 1, "the previous hook ran");
    let fault = fault_with(&log, "boom 1");
    assert_eq!(fault["level"], "error");
    assert_eq!(fault["fields"]["where"], "worker-7");
    let line = LINE.load(Ordering::SeqCst) + 1;
    let location = fault["fields"]["location"].as_str().unwrap();
    assert!(
        location.ends_with(&format!("faults_tests.rs:{line}")),
        "{location}"
    );
    assert!(
        crate::logging::events_tests::validator().is_valid(&fault),
        "{fault}"
    );
}

#[test]
fn a_static_string_payload_is_the_message_and_any_other_payload_is_not_a_string() {
    let dir = tempfile::tempdir().unwrap();
    let (log, _) = hooked(&dir, true);
    panics_on("worker-8", || panic!("a static text"));
    assert_eq!(
        fault_with(&log, "a static text")["fields"]["where"],
        "worker-8"
    );
    panics_on("worker-9", || std::panic::panic_any(42u32));
    let other = records(&log)
        .into_iter()
        .find(|r| r["event"] == "fault" && r["fields"]["where"] == "worker-9")
        .expect("a fault for the thread");
    assert_eq!(other["fields"]["message"], "non-string payload");
}

#[test]
fn a_panic_in_a_unit_of_work_is_named_by_the_unit_and_an_unnamed_thread_is_called_so() {
    let dir = tempfile::tempdir().unwrap();
    let (log, _) = hooked(&dir, true);
    let (tx, _rx) = events();
    let result = run_unit(&tx, "reconcile", || panic!("inside the unit"));
    assert_eq!(result, Err::<(), _>(Faulted));
    assert_eq!(
        fault_with(&log, "inside the unit")["fields"]["where"],
        "reconcile"
    );
    // After the unit the thread is called by its own name again.
    let outside = std::thread::spawn(|| panic!("outside any unit")).join();
    assert!(outside.is_err());
    assert_eq!(
        fault_with(&log, "outside any unit")["fields"]["where"],
        "<unnamed>"
    );
}

#[tokio::test]
async fn a_panic_in_an_async_unit_is_named_by_the_unit() {
    let dir = tempfile::tempdir().unwrap();
    let (log, _) = hooked(&dir, true);
    let (tx, _rx) = events();
    let value = run_unit_async(&tx, "control", async {
        tokio::task::yield_now().await;
        if true {
            panic!("inside the async unit");
        }
    })
    .await;
    assert_eq!(value, Err(Faulted));
    assert_eq!(
        fault_with(&log, "inside the async unit")["fields"]["where"],
        "control"
    );
}

#[test]
fn nothing_is_written_while_logging_is_off() {
    let dir = tempfile::tempdir().unwrap();
    let (log, calls) = hooked(&dir, false);
    panics_on("worker-10", || panic!("quietly"));
    assert!(calls.load(Ordering::SeqCst) >= 1);
    assert!(!log.path().exists());
}

/// The records of `node`'s log named `event` at `at`, with their positions.
fn positions(node_log: &Log, event: &str, at: &str) -> Vec<(usize, Value)> {
    records(node_log)
        .into_iter()
        .enumerate()
        .filter(|(_, r)| r["event"] == event && r["fields"]["where"] == at)
        .collect()
}

/// The fault at `at` and then its recovery with `action` are in the log.
async fn faulted_and_recovered(node: &TestNode, at: &str, action: &str) {
    let log = node.engine.log();
    eventually("the recovery to be logged", || async {
        !positions(&log, "fault.recovered", at).is_empty()
    })
    .await;
    let fault = positions(&log, "fault", at);
    let recovered = positions(&log, "fault.recovered", at);
    assert!(!fault.is_empty(), "{:?}", records(&log));
    assert_eq!(recovered[0].1["fields"]["action"], action);
    assert_eq!(recovered[0].1["level"], "warn");
    assert!(
        fault[0].0 < recovered[0].0,
        "the fault comes before it was recovered from"
    );
}

#[tokio::test]
async fn a_reconcile_tick_that_faulted_returned_control_here() {
    let lab = Lab::new();
    let (alpha, knobs) = lab.start_flaky("Alpha").await;
    let _: mouser_proto::control::LoggingSetResult = call(
        &alpha,
        mouser_proto::control::Method::LoggingSet,
        json!({"enabled": true}),
    )
    .await;
    let nodes = desk_from(vec![alpha, lab.start("Beta").await]).await;
    install(nodes[0].engine.log());
    push_right(&nodes[0]);
    eventually("the pointer to leave", || async {
        nodes[0].platform.pointer_hidden()
    })
    .await;
    knobs.panic_secure_input.store(true, Ordering::SeqCst);
    faulted_and_recovered(&nodes[0], "reconcile", "returned_local").await;
}

#[tokio::test]
async fn an_inbound_message_that_faulted_closed_the_link() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    install(b.engine.log());
    b.platform.panic_on_next_inject();
    send_to(
        a,
        b,
        mouser_proto::wire::ControlMessage::Enter {
            display_id: mouser_proto::model::DisplayId {
                computer: b.engine.computer_id(),
                key: "virtual-0".into(),
            },
            x: 1.0,
            y: 2.0,
            seq_base: 1,
            modifiers_down: mouser_proto::wire::Modifiers::default(),
        },
    );
    faulted_and_recovered(b, "read_loop", "closed_link").await;
}

#[tokio::test]
async fn a_datagram_that_faulted_closed_the_link() {
    use mouser_proto::wire::{MotionDatagram, display_id_hash};
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    install(b.engine.log());
    push_right(a);
    injected_where(&b.platform, "entry", |i| {
        matches!(i, mouser_testkit::Injected::MotionAbs { .. })
    })
    .await;
    b.platform.panic_on_next_inject();
    datagram_to(
        a,
        b,
        &MotionDatagram {
            seq: 1000,
            display_id_hash: display_id_hash(&mouser_proto::model::DisplayId {
                computer: b.engine.computer_id(),
                key: "virtual-0".into(),
            }),
            x: 5.0,
            y: 6.0,
        },
    );
    faulted_and_recovered(b, "motion_loop", "closed_link").await;
}

#[tokio::test]
async fn an_input_event_that_faulted_returned_control_here() {
    use mouser_platform::LocalInput;
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let a = &nodes[0];
    install(a.engine.log());
    // A held key is released locally before the pointer leaves: that inject
    // is the one that panics.
    a.platform.send_input(LocalInput::Key {
        usage: 0x0007_0004,
        down: true,
    });
    settle().await;
    a.platform.panic_on_next_inject();
    push_right(a);
    faulted_and_recovered(a, "mouser-input", "returned_local").await;
}

#[tokio::test]
async fn a_control_request_that_faulted_carried_on() {
    use mouser_platform::{Os, PlatformInfo, SessionType};
    let platform = mouser_testkit::VirtualPlatform::new();
    platform.set_info(PlatformInfo {
        os: Os::Windows,
        version: "virtual".into(),
        session: SessionType::Native,
        limitations: Vec::new(),
    });
    platform.set_secure_attention_supported(true);
    let lab = Lab::new();
    let node = lab
        .start_with("Alpha", platform, |c| c.diagnostics.logging = true)
        .await;
    install(node.engine.log());
    node.platform.panic_on_next_inject();
    let err = call_err(&node, mouser_proto::control::Method::SendCad, json!({})).await;
    assert_eq!(err.code, -32603);
    faulted_and_recovered(&node, "control", "continued").await;
}
