use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::channel;

use mouser_proto::service_ipc::{EngineMessage, SasResult, ServiceMessage, engine_pipe_name};

use crate::fake::{Call, FakeWin};
use crate::launch::{Answer, launch};
use crate::session::Event;
use crate::winapi::{Handle, WinApi};

const DIR: &str = r"C:\Program Files\Mouser";

fn sent() -> Answer {
    Arc::new(|| SasResult::Sent)
}

fn api(fake: &Arc<FakeWin>) -> Arc<dyn WinApi> {
    fake.clone()
}

#[test]
fn the_engine_runs_beside_the_service_on_the_sessions_winlogon_token_and_default_desktop() {
    let fake = Arc::new(FakeWin::new());
    // Another session's winlogon.exe is not the one.
    fake.start_process("winlogon.exe", 2);
    let winlogon = fake.start_process("WINLOGON.EXE", 3);
    let (events, _heard) = channel();
    let engine = launch(&api(&fake), 3, Path::new(DIR), sent(), events).unwrap();
    let calls = fake.calls();
    let Call::CreateProcess { token, .. } = calls[2].clone() else {
        panic!("{calls:?}")
    };
    assert_eq!(
        calls[..4],
        [
            Call::CreatePipe {
                name: engine_pipe_name(3),
                sddl: crate::ipc::SDDL.into(),
                open_mode: 0x4008_0003,
                pipe_mode: 0x0000_0008,
            },
            Call::DuplicateToken(winlogon),
            Call::CreateProcess {
                token,
                application: Path::new(DIR).join("mouser-app.exe"),
                command_line: format!(r#""{DIR}\mouser-app.exe" --headless --machine"#),
                desktop: r"winsta0\default".into(),
            },
            Call::Close(token),
        ]
    );
    assert!(fake.running().contains(&("mouser-app.exe".into(), 3)));
    assert_eq!(engine.session, 3);
    fake.exit(engine.pid, 0);
    assert_eq!(engine.finish(), 0);
}

#[test]
fn the_engine_is_served_and_its_exit_is_heard() {
    let fake = Arc::new(FakeWin::new());
    let (events, heard) = channel();
    let engine = launch(&api(&fake), 1, Path::new(DIR), sent(), events).unwrap();
    let name = engine_pipe_name(1);
    fake.connect(&name, engine.pid);
    fake.send(&name, &EngineMessage::SendSas);
    assert_eq!(
        fake.await_received(&name, 1),
        [ServiceMessage::SasResult(SasResult::Sent)]
    );
    fake.exit(engine.pid, 7);
    assert_eq!(
        heard.recv().unwrap(),
        Event::EngineExited { pid: engine.pid }
    );
    assert_eq!(engine.finish(), 7);
}

#[test]
fn shutdown_tells_the_engine_and_finishing_waits_for_it_and_lets_go() {
    let fake = Arc::new(FakeWin::new());
    let (events, _heard) = channel();
    let engine = launch(&api(&fake), 1, Path::new(DIR), sent(), events).unwrap();
    let name = engine_pipe_name(1);
    fake.connect(&name, engine.pid);
    engine.shutdown();
    assert_eq!(engine.finish(), 0);
    assert_eq!(fake.received(&name), [ServiceMessage::Shutdown]);
    assert!(!fake.has_pipe(&name));
    assert!(!fake.running().contains(&("mouser-app.exe".into(), 1)));
}

#[test]
fn an_engine_that_cannot_be_waited_for_ends_with_an_unknown_code() {
    let fake = Arc::new(FakeWin::new());
    // The watcher's wait fails first: it reports the engine gone.
    fake.fail("wait_process", 6);
    let (events, heard) = channel();
    let engine = launch(&api(&fake), 1, Path::new(DIR), sent(), events).unwrap();
    assert_eq!(
        heard.recv().unwrap(),
        Event::EngineExited { pid: engine.pid }
    );
    fake.fail("wait_process", 6);
    assert_eq!(engine.finish(), u32::MAX);
}

#[test]
fn nothing_is_left_open_when_the_engine_cannot_be_started() {
    for (fail, code) in [
        ("create_pipe", 5),
        ("processes", 5),
        ("duplicate_token", 5),
        ("create_process_as_user", 2),
    ] {
        let fake = Arc::new(FakeWin::new());
        fake.fail(fail, code);
        let (events, _heard) = channel();
        let failed = launch(&api(&fake), 1, Path::new(DIR), sent(), events).err();
        assert_eq!(failed.map(|e| e.code), Some(code), "{fail}");
        assert!(!fake.has_pipe(&engine_pipe_name(1)), "{fail}");
        let tokens: Vec<Handle> = fake
            .calls()
            .iter()
            .filter_map(|c| match c {
                Call::CreateProcess { token, .. } => Some(*token),
                _ => None,
            })
            .collect();
        for token in tokens {
            assert!(fake.calls().contains(&Call::Close(token)), "{fail}");
        }
    }
}

#[test]
fn a_session_without_winlogon_cannot_have_an_engine() {
    let fake = Arc::new(FakeWin::new());
    let (events, _heard) = channel();
    let failed = launch(&api(&fake), 9, Path::new(DIR), sent(), events)
        .err()
        .unwrap();
    assert_eq!(failed.code, 1168);
    assert_eq!(failed.message, "session 9 has no winlogon.exe");
}
