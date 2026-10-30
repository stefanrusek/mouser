use crate::service_client::*;
use mouser_platform::Error;
use mouser_proto::service_ipc::{EngineMessage, SasResult, ServiceMessage, StopReason};
use mouser_proto::wire::{decode_frame, encode_frame};
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::channel;
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::core::HSTRING;

/// A pipe name no other test uses, so tests running in parallel never reach
/// each other's fake service.
pub(crate) fn unique_name() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let n = NEXT.fetch_add(1, Ordering::SeqCst);
    format!(r"\\.\pipe\mouser-test-svc-{}-{n}", std::process::id())
}

/// The service's end of the pipe, as the service talks on it.
pub(crate) struct ServiceEnd(File);

impl ServiceEnd {
    pub(crate) fn read(&mut self) -> Option<EngineMessage> {
        let mut len = [0u8; 4];
        self.0.read_exact(&mut len).ok()?;
        let mut frame = len.to_vec();
        frame.resize(4 + u32::from_be_bytes(len) as usize, 0);
        self.0.read_exact(&mut frame[4..]).ok()?;
        Some(decode_frame(&frame).unwrap().0)
    }

    pub(crate) fn send(&mut self, msg: &ServiceMessage) {
        self.0.write_all(&encode_frame(msg)).unwrap();
    }
}

/// A fake service on `name`: it waits for the engine, then follows
/// `script`. The pipe exists before this returns, so the engine can connect
/// at once. Join the thread at the end of the test.
pub(crate) fn serve<T: Send + 'static>(
    name: &str,
    script: impl FnOnce(&mut ServiceEnd) -> T + Send + 'static,
) -> JoinHandle<T> {
    let pipe = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(name),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            None,
        )
    };
    assert!(!pipe.is_invalid(), "the fake service's pipe");
    let raw = pipe.0 as usize;
    std::thread::spawn(move || {
        let pipe = windows::Win32::Foundation::HANDLE(raw as *mut _);
        // A client that came before the wait is connected all the same.
        let _ = unsafe { ConnectNamedPipe(pipe, None) };
        let mut end = ServiceEnd(unsafe { File::from_raw_handle(pipe.0) });
        script(&mut end)
    })
}

#[test]
fn the_engine_says_hello_with_its_version_first() {
    let name = unique_name();
    let service = serve(&name, |end| end.read());
    let client = connect_to(&name, "0.3.0").unwrap();
    let first = service.join().unwrap();
    drop(client);
    assert_eq!(
        first,
        Some(EngineMessage::Hello {
            version: "0.3.0".into()
        })
    );
}

#[test]
fn a_pipe_with_no_service_is_an_os_error() {
    let Err(Error::Os(text)) = connect_to(&unique_name(), "0.3.0") else {
        panic!("connected to nothing");
    };
    assert!(text.starts_with("the Mouser service"), "{text}");
}

#[test]
fn stopping_tells_the_service_why() {
    let name = unique_name();
    let service = serve(&name, |end| {
        end.read();
        (end.read(), end.read())
    });
    let client = connect_to(&name, "0.3.0").unwrap();
    client.stopping(StopReason::Quit).unwrap();
    client.stopping(StopReason::Fault).unwrap();
    let said = service.join().unwrap();
    drop(client);
    assert_eq!(
        said,
        (
            Some(EngineMessage::Stopping {
                reason: StopReason::Quit
            }),
            Some(EngineMessage::Stopping {
                reason: StopReason::Fault
            })
        )
    );
}

#[test]
fn shutdown_from_the_service_runs_what_was_registered_once() {
    let name = unique_name();
    let (go_tx, go) = channel::<()>();
    let service = serve(&name, move |end| {
        end.read();
        go.recv().unwrap();
        end.send(&ServiceMessage::Shutdown);
        // A second one runs nothing more.
        end.send(&ServiceMessage::Shutdown);
        end.read()
    });
    let client = connect_to(&name, "0.3.0").unwrap();
    let (ran_tx, ran) = channel();
    client.on_shutdown(Box::new(move || ran_tx.send(()).unwrap()));
    go_tx.send(()).unwrap();
    assert_eq!(ran.recv_timeout(Duration::from_secs(5)), Ok(()));
    // The engine exits voluntarily after: the service hears it.
    client.stopping(StopReason::Quit).unwrap();
    let after = service.join().unwrap();
    drop(client);
    assert!(ran.try_recv().is_err(), "it ran once");
    assert_eq!(
        after,
        Some(EngineMessage::Stopping {
            reason: StopReason::Quit
        })
    );
}

#[test]
fn a_shutdown_that_came_before_anything_was_registered_runs_it_at_once() {
    let name = unique_name();
    let service = serve(&name, |end| {
        end.read();
        end.send(&ServiceMessage::Shutdown);
        // Waits for the engine to go.
        end.read()
    });
    let client = connect_to(&name, "0.3.0").unwrap();
    client.wait_for_shutdown_for_tests();
    let (ran_tx, ran) = channel();
    client.on_shutdown(Box::new(move || ran_tx.send(()).unwrap()));
    assert_eq!(ran.recv_timeout(Duration::from_secs(5)), Ok(()));
    drop(client);
    assert_eq!(service.join().unwrap(), None, "the engine went");
}

#[test]
fn ctrl_alt_del_is_sent_only_when_the_service_says_it_was() {
    let name = unique_name();
    let answers = [
        SasResult::Sent,
        SasResult::PolicyBlocked,
        SasResult::Failed("access denied".into()),
    ];
    let replies = answers.clone();
    let service = serve(&name, move |end| {
        end.read();
        let mut asked = Vec::new();
        for answer in replies {
            asked.push(end.read());
            end.send(&ServiceMessage::SasResult(answer));
        }
        asked
    });
    let client = connect_to(&name, "0.3.0").unwrap();
    let results: Vec<_> = answers.iter().map(|_| client.send_sas()).collect();
    let asked = service.join().unwrap();
    drop(client);
    assert_eq!(asked, vec![Some(EngineMessage::SendSas); 3]);
    assert_eq!(results[0], Ok(()));
    assert_eq!(
        results[1],
        Err(Error::Os(
            "Ctrl-Alt-Del is blocked by this PC's policy".into()
        ))
    );
    assert_eq!(
        results[2],
        Err(Error::Os(
            "the Mouser service could not send Ctrl-Alt-Del: access denied".into()
        ))
    );
}

#[test]
fn once_the_service_is_gone_requests_are_os_errors_and_nothing_else() {
    let name = unique_name();
    // The service hears hello and goes away.
    let service = serve(&name, |end| end.read());
    let client = connect_to(&name, "0.3.0").unwrap();
    service.join().unwrap();
    client.wait_for_shutdown_for_tests();
    assert!(matches!(client.send_sas(), Err(Error::Os(_))));
    assert!(matches!(
        client.stopping(StopReason::Quit),
        Err(Error::Os(_))
    ));
    drop(client);
}

#[test]
fn a_service_that_goes_before_it_answers_is_an_os_error() {
    let name = unique_name();
    // Hears hello and the request, and goes without a word.
    let service = serve(&name, |end| {
        end.read();
        end.read()
    });
    let client = connect_to(&name, "0.3.0").unwrap();
    let result = client.send_sas();
    let asked = service.join().unwrap();
    drop(client);
    assert_eq!(asked, Some(EngineMessage::SendSas));
    assert_eq!(result, Err(Error::Os("the Mouser service is gone".into())));
}

#[test]
fn the_engine_pipe_is_named_for_the_console_session() {
    use windows::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId;
    assert_eq!(console_session_id(), unsafe {
        WTSGetActiveConsoleSessionId()
    });
    assert_eq!(
        engine_pipe(),
        mouser_proto::service_ipc::engine_pipe_name(console_session_id())
    );
}
