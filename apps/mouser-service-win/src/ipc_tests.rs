use std::sync::Arc;

use mouser_proto::service_ipc::{
    EngineMessage, SasResult, ServiceMessage, StopReason, engine_pipe_name,
};

use crate::fake::{Call, FakeWin};
use crate::ipc::Pipe;
use crate::winapi::WinApi;

fn pipe_in(fake: &Arc<FakeWin>, session: u32) -> Arc<Pipe> {
    Arc::new(Pipe::create(fake.clone() as Arc<dyn WinApi>, session).unwrap())
}

/// Serves `engine` on `pipe` in a thread, answering every `SendSas` with `Sent`.
fn serving(pipe: &Arc<Pipe>, engine: u32) -> std::thread::JoinHandle<()> {
    let pipe = pipe.clone();
    std::thread::spawn(move || pipe.serve(engine, &|| SasResult::Sent))
}

#[test]
fn the_pipe_is_the_sessions_and_only_system_on_this_computer_can_open_it() {
    let fake = Arc::new(FakeWin::new());
    let _pipe = pipe_in(&fake, 3);
    assert_eq!(
        fake.calls(),
        [Call::CreatePipe {
            name: r"\\.\pipe\mouser-svc-3".into(),
            sddl: "D:P(A;;GA;;;SY)".into(),
            // PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED
            open_mode: 0x0000_0003 | 0x0008_0000 | 0x4000_0000,
            // PIPE_TYPE_BYTE (0) | PIPE_REJECT_REMOTE_CLIENTS
            pipe_mode: 0x0000_0008,
        }]
    );
}

#[test]
fn a_pipe_that_cannot_be_created_says_why() {
    let fake = Arc::new(FakeWin::new());
    fake.fail("create_pipe", 5);
    let failed = Pipe::create(fake.clone() as Arc<dyn WinApi>, 1)
        .err()
        .unwrap();
    assert_eq!(failed.code, 5);
}

#[test]
fn a_sessions_pipe_is_its_first_and_only_instance() {
    let fake = Arc::new(FakeWin::new());
    let _pipe = pipe_in(&fake, 1);
    let second = Pipe::create(fake.clone() as Arc<dyn WinApi>, 1)
        .err()
        .unwrap();
    assert_eq!(second.code, 5);
}

#[test]
fn a_client_that_is_not_the_launched_engine_is_disconnected_and_the_engine_is_served() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let name = engine_pipe_name(1);
    let engine = fake.start_process("mouser-app.exe", 1);
    let stranger = fake.start_process("other.exe", 1);
    let served = serving(&pipe, engine);
    fake.connect(&name, stranger);
    fake.await_calls(|calls| calls.contains(&Call::Disconnect(pipe.handle())));
    fake.connect(&name, engine);
    fake.send(
        &name,
        &EngineMessage::Hello {
            version: env!("CARGO_PKG_VERSION").into(),
        },
    );
    fake.send(&name, &EngineMessage::SendSas);
    assert_eq!(
        fake.await_received(&name, 1),
        [ServiceMessage::SasResult(SasResult::Sent)]
    );
    fake.exit(engine, 0);
    served.join().unwrap();
}

#[test]
fn an_engine_of_another_version_still_gets_ctrl_alt_del_and_may_say_it_is_stopping() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let name = engine_pipe_name(1);
    let engine = fake.start_process("mouser-app.exe", 1);
    let served = serving(&pipe, engine);
    fake.connect(&name, engine);
    fake.send(
        &name,
        &EngineMessage::Hello {
            version: "0.0.1".into(),
        },
    );
    fake.send(&name, &EngineMessage::SendSas);
    fake.send(
        &name,
        &EngineMessage::Stopping {
            reason: StopReason::Quit,
        },
    );
    fake.send(&name, &EngineMessage::SendSas);
    assert_eq!(
        fake.await_received(&name, 2),
        [
            ServiceMessage::SasResult(SasResult::Sent),
            ServiceMessage::SasResult(SasResult::Sent)
        ]
    );
    fake.exit(engine, 0);
    served.join().unwrap();
}

#[test]
fn shutdown_goes_to_a_connected_engine_at_once() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let name = engine_pipe_name(1);
    let engine = fake.start_process("mouser-app.exe", 1);
    let served = serving(&pipe, engine);
    fake.connect(&name, engine);
    fake.send(&name, &EngineMessage::SendSas);
    fake.await_received(&name, 1);
    pipe.shutdown();
    assert_eq!(
        fake.received(&name),
        [
            ServiceMessage::SasResult(SasResult::Sent),
            ServiceMessage::Shutdown
        ]
    );
    served.join().unwrap();
}

#[test]
fn shutdown_waits_for_an_engine_that_has_not_connected_yet() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let name = engine_pipe_name(1);
    let engine = fake.start_process("mouser-app.exe", 1);
    pipe.shutdown();
    assert_eq!(fake.received(&name), []);
    let served = serving(&pipe, engine);
    fake.connect(&name, engine);
    assert_eq!(fake.await_received(&name, 1), [ServiceMessage::Shutdown]);
    served.join().unwrap();
}

#[test]
fn a_message_that_is_not_one_ends_the_conversation() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let name = engine_pipe_name(1);
    let engine = fake.start_process("mouser-app.exe", 1);
    let served = serving(&pipe, engine);
    fake.connect(&name, engine);
    fake.send_bytes(&name, &[0, 0, 0, 1, 0xFF]);
    served.join().unwrap();
    assert_eq!(fake.received(&name), []);
}

#[test]
fn an_engine_that_goes_before_it_connects_ends_the_wait_even_one_not_begun() {
    let fake = Arc::new(FakeWin::new());
    // Ended before the wait begins.
    let pipe = pipe_in(&fake, 1);
    pipe.end();
    serving(&pipe, 4242).join().unwrap();
    drop(pipe);
    // Ended while it waits.
    let pipe = pipe_in(&fake, 1);
    let served = serving(&pipe, 4242);
    pipe.end();
    served.join().unwrap();
}

#[test]
fn ending_a_pipe_whose_engine_is_connected_or_that_cannot_wait_changes_nothing() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let engine = fake.start_process("mouser-app.exe", 1);
    fake.connect(&engine_pipe_name(1), engine);
    pipe.end();
    fake.fail("connect_pipe", 6);
    serving(&pipe, engine).join().unwrap();
}

#[test]
fn a_client_whose_process_cannot_be_told_is_a_stranger_and_one_that_stays_ends_the_wait() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let name = engine_pipe_name(1);
    let engine = fake.start_process("mouser-app.exe", 1);
    fake.fail("pipe_client_pid", 6);
    fake.fail("disconnect_pipe", 6);
    let served = serving(&pipe, engine);
    fake.connect(&name, engine);
    served.join().unwrap();
    assert!(fake.calls().contains(&Call::Disconnect(pipe.handle())));
}

#[test]
fn a_failed_answer_ends_the_conversation() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let name = engine_pipe_name(1);
    let engine = fake.start_process("mouser-app.exe", 1);
    fake.fail("write_pipe", 232);
    let served = serving(&pipe, engine);
    fake.connect(&name, engine);
    fake.send(&name, &EngineMessage::SendSas);
    served.join().unwrap();
}

#[test]
fn the_pipe_is_closed_when_dropped() {
    let fake = Arc::new(FakeWin::new());
    let pipe = pipe_in(&fake, 1);
    let handle = pipe.handle();
    drop(pipe);
    assert_eq!(fake.calls().last(), Some(&Call::Close(handle)));
    assert!(!fake.has_pipe(&engine_pipe_name(1)));
}
