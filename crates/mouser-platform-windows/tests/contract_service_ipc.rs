//! Contract test (consumer side) for contracts/service-ipc.md: the session
//! engine's client, through its frozen API, talks to a stand-in service on
//! the pipe the service makes for the console session. Every frame each way
//! is the 4-byte big-endian length and postcard body of `mouser_proto::wire`,
//! decoded and encoded with the `mouser_proto::service_ipc` types: `Hello`,
//! `SendSas` and `Stopping` from the engine, `SasResult` and `Shutdown` from
//! the service.

#![cfg(windows)]

use mouser_platform::{Error, Platform};
use mouser_platform_windows::service_client::{ServiceClient, console_session_id};
use mouser_platform_windows::{Settings, WinPlatform};
use mouser_proto::service_ipc::{
    EngineMessage, SasResult, ServiceMessage, StopReason, engine_pipe_name,
};
use mouser_proto::wire::{decode_frame, encode_frame};
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::io::FromRawHandle;
use std::sync::mpsc::channel;
use std::time::Duration;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::core::HSTRING;

/// Reads one frame and checks its framing: the length prefix covers the
/// body exactly.
fn read_frame(pipe: &mut File) -> EngineMessage {
    let mut len = [0u8; 4];
    pipe.read_exact(&mut len).unwrap();
    let body = u32::from_be_bytes(len) as usize;
    let mut frame = len.to_vec();
    frame.resize(4 + body, 0);
    pipe.read_exact(&mut frame[4..]).unwrap();
    let (msg, used) = decode_frame(&frame).unwrap();
    assert_eq!(used, frame.len(), "one frame, all of it");
    msg
}

fn write_frame(pipe: &mut File, msg: &ServiceMessage) {
    pipe.write_all(&encode_frame(msg)).unwrap();
}

#[test]
fn the_engine_and_the_service_speak_the_contracts_frames() {
    let name = engine_pipe_name(console_session_id());
    let pipe = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(name.as_str()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
            1,
            4096,
            4096,
            0,
            None,
        )
    };
    assert!(!pipe.is_invalid(), "the stand-in service's pipe {name}");
    let raw = pipe.0 as usize;
    let (go_tx, go) = channel::<()>();
    let service = std::thread::spawn(move || {
        let pipe = HANDLE(raw as *mut _);
        let _ = unsafe { ConnectNamedPipe(pipe, None) };
        let mut pipe = unsafe { File::from_raw_handle(pipe.0) };
        let mut heard = vec![read_frame(&mut pipe), read_frame(&mut pipe)];
        for answer in [
            SasResult::Sent,
            SasResult::PolicyBlocked,
            SasResult::Failed("x".into()),
        ] {
            heard.push(read_frame(&mut pipe));
            write_frame(&mut pipe, &ServiceMessage::SasResult(answer));
        }
        go.recv().unwrap();
        write_frame(&mut pipe, &ServiceMessage::Shutdown);
        heard
    });

    let client = ServiceClient::connect("0.3.0").unwrap();
    let (ran_tx, ran) = channel();
    client.on_shutdown(Box::new(move || ran_tx.send(()).unwrap()));
    client.stopping(StopReason::Quit).unwrap();
    let platform = WinPlatform::with_service_client(Settings::system(), client);
    let mut inject = platform.inject().unwrap();
    let sas: Vec<_> = (0..3).map(|_| inject.secure_attention()).collect();
    go_tx.send(()).unwrap();
    let shut = ran.recv_timeout(Duration::from_secs(5));
    drop((inject, platform));
    let heard = service.join().unwrap();

    assert_eq!(sas[0], Ok(()));
    assert!(matches!(sas[1], Err(Error::Os(_))));
    assert!(matches!(sas[2], Err(Error::Os(_))));
    assert_eq!(shut, Ok(()));
    assert_eq!(
        heard,
        [
            EngineMessage::Hello {
                version: "0.3.0".into()
            },
            EngineMessage::Stopping {
                reason: StopReason::Quit
            },
            EngineMessage::SendSas,
            EngineMessage::SendSas,
            EngineMessage::SendSas,
        ]
    );
}
