use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};

use mouser_proto::service_ipc::{SasResult, ServiceMessage, engine_pipe_name};

use crate::fake::{FakeWin, NO_SESSION};
use crate::session::{Event, State, Supervisor};
use crate::test_log::TestLog;
use crate::winapi::WinApi;

struct Rig {
    fake: Arc<FakeWin>,
    log: TestLog,
    supervisor: Supervisor,
    heard: Receiver<Event>,
}

fn rig() -> Rig {
    let fake = Arc::new(FakeWin::new());
    let log = TestLog::new();
    let (events, heard) = channel();
    let supervisor = Supervisor::new(
        fake.clone() as Arc<dyn WinApi>,
        log.log.clone(),
        Path::new(r"C:\Program Files\Mouser").into(),
        Arc::new(|| SasResult::Sent),
        events,
    );
    Rig {
        fake,
        log,
        supervisor,
        heard,
    }
}

/// The running engine's session and process, connected to its pipe as a
/// session engine does once it has started.
fn running(r: &Rig) -> (u32, u32) {
    match r.supervisor.state() {
        State::Running { session, pid } => {
            r.fake.connect(&engine_pipe_name(session), pid);
            (session, pid)
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn with_no_one_at_the_console_it_waits_for_a_session_change_and_then_runs_the_engine() {
    let mut r = rig();
    r.fake.set_console(NO_SESSION);
    r.supervisor.start();
    assert_eq!(r.supervisor.state(), State::WaitingForSession);
    // Nothing is created while it waits, and it does not look again by itself.
    assert_eq!(r.fake.calls(), []);
    // A change that still leaves no console session keeps it waiting.
    r.supervisor.on(Event::SessionChange);
    assert_eq!(r.supervisor.state(), State::WaitingForSession);
    r.fake.start_process("winlogon.exe", 2);
    r.fake.set_console(2);
    r.supervisor.on(Event::SessionChange);
    let (session, pid) = running(&r);
    assert_eq!(session, 2);
    assert_eq!(
        r.supervisor.states(),
        [
            State::Starting,
            State::WaitingForSession,
            State::Running { session: 2, pid }
        ]
    );
    assert_eq!(
        r.log.events(),
        [
            r#"service.session {"session":2}"#.to_string(),
            format!(r#"service.engine_started {{"pid":{pid},"session":2}}"#),
        ]
    );
    r.supervisor.on(Event::Stop);
}

#[test]
fn a_console_session_change_shuts_the_engine_down_and_starts_one_in_the_new_session() {
    let mut r = rig();
    r.supervisor.start();
    let (_, old) = running(&r);
    r.fake.start_process("winlogon.exe", 2);
    r.fake.set_console(2);
    r.supervisor.on(Event::SessionChange);
    let (session, new) = running(&r);
    assert_eq!(session, 2);
    assert_eq!(
        r.fake.received(&engine_pipe_name(1)),
        [ServiceMessage::Shutdown]
    );
    assert!(!r.fake.has_pipe(&engine_pipe_name(1)));
    assert!(r.fake.has_pipe(&engine_pipe_name(2)));
    assert_eq!(
        r.supervisor.states()[1..],
        [
            State::Running {
                session: 1,
                pid: old
            },
            State::Relaunching,
            State::Running {
                session: 2,
                pid: new
            }
        ]
    );
    assert_eq!(
        r.log.events()[2..],
        [
            r#"service.engine_exited {"code":0}"#.to_string(),
            r#"service.session {"session":2}"#.to_string(),
            format!(r#"service.engine_started {{"pid":{new},"session":2}}"#),
        ]
    );
    // The old engine's end, heard late, is not this engine's.
    r.supervisor.on(r.heard.recv().unwrap());
    assert_eq!(running(&r), (2, new));
    r.supervisor.on(Event::Stop);
}

#[test]
fn an_engine_that_exits_is_started_again_at_once_in_the_same_session() {
    let mut r = rig();
    r.supervisor.start();
    let (_, first) = running(&r);
    r.fake.exit(first, 3);
    r.supervisor.on(r.heard.recv().unwrap());
    let (session, second) = running(&r);
    assert_eq!(session, 1);
    assert_ne!(second, first);
    assert_eq!(
        r.supervisor.states()[2..],
        [
            State::Relaunching,
            State::Running {
                session: 1,
                pid: second
            }
        ]
    );
    assert_eq!(
        r.log.events()[2..],
        [
            r#"service.engine_exited {"code":3}"#.to_string(),
            format!(r#"service.engine_started {{"pid":{second},"session":1}}"#),
        ]
    );
    r.supervisor.on(Event::Stop);
}

#[test]
fn stopping_shuts_the_engine_down_and_waits_for_it() {
    let mut r = rig();
    r.supervisor.start();
    running(&r);
    r.supervisor.on(Event::Stop);
    assert_eq!(r.supervisor.state(), State::Stopped);
    assert_eq!(
        r.supervisor.states()[2..],
        [State::Stopping, State::Stopped]
    );
    assert_eq!(
        r.fake.received(&engine_pipe_name(1)),
        [ServiceMessage::Shutdown]
    );
    assert!(
        r.fake
            .running()
            .iter()
            .all(|(name, _)| name != "mouser-app.exe")
    );
    assert_eq!(
        r.log.events().last().unwrap(),
        r#"service.engine_exited {"code":0}"#
    );
}

#[test]
fn stopping_while_waiting_for_a_session_just_stops() {
    let mut r = rig();
    r.fake.set_console(NO_SESSION);
    r.supervisor.start();
    r.supervisor.on(Event::Stop);
    assert_eq!(
        r.supervisor.states(),
        [State::Starting, State::WaitingForSession, State::Stopped]
    );
}

#[test]
fn an_engine_that_cannot_be_started_waits_for_the_next_session_change() {
    let mut r = rig();
    r.fake.fail("create_process_as_user", 2);
    r.supervisor.start();
    assert_eq!(r.supervisor.state(), State::WaitingForSession);
    assert!(!r.fake.has_pipe(&engine_pipe_name(1)));
    r.supervisor.on(Event::SessionChange);
    running(&r);
    r.supervisor.on(Event::Stop);
}

#[test]
fn the_supervisor_runs_until_it_is_told_to_stop() {
    let r = rig();
    let (events, heard) = channel();
    let fake = r.fake.clone();
    let run = std::thread::spawn(move || {
        let mut supervisor = r.supervisor;
        supervisor.run(heard);
        supervisor.states()
    });
    let first = fake.connect_engine(1);
    events.send(Event::SessionChange).unwrap();
    let second = fake.connect_engine(1);
    assert_ne!(first, second);
    events.send(Event::Stop).unwrap();
    let states = run.join().unwrap();
    assert_eq!(
        states[1..],
        [
            State::Running {
                session: 1,
                pid: first
            },
            State::Relaunching,
            State::Running {
                session: 1,
                pid: second
            },
            State::Stopping,
            State::Stopped
        ]
    );
}

#[test]
fn a_supervisor_whose_events_end_stops() {
    let r = rig();
    let (events, heard) = channel();
    let fake = r.fake.clone();
    let run = std::thread::spawn(move || {
        let mut supervisor = r.supervisor;
        supervisor.run(heard);
        supervisor.state()
    });
    fake.connect_engine(1);
    drop(events);
    assert_eq!(run.join().unwrap(), State::Stopped);
}
