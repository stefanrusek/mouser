use mouser_proto::service_ipc::*;
use mouser_proto::wire::{FrameError, decode_frame, encode_frame};

#[test]
fn engine_messages_roundtrip_through_framing() {
    let msgs = [
        EngineMessage::Hello {
            version: "0.3.0".into(),
        },
        EngineMessage::SendSas,
        EngineMessage::Stopping {
            reason: StopReason::Quit,
        },
        EngineMessage::Stopping {
            reason: StopReason::Fault,
        },
    ];
    for m in msgs {
        let frame = encode_frame(&m);
        let len = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
        assert_eq!(len, frame.len() - 4);
        assert_eq!(
            decode_frame::<EngineMessage>(&frame).unwrap(),
            (m, frame.len())
        );
    }
}

#[test]
fn service_messages_roundtrip_through_framing() {
    let msgs = [
        ServiceMessage::SasResult(SasResult::Sent),
        ServiceMessage::SasResult(SasResult::PolicyBlocked),
        ServiceMessage::SasResult(SasResult::Failed("access denied".into())),
        ServiceMessage::Shutdown,
    ];
    let mut stream = Vec::new();
    for m in &msgs {
        stream.extend(encode_frame(m));
    }
    let mut rest = &stream[..];
    for m in &msgs {
        let (back, used) = decode_frame::<ServiceMessage>(rest).unwrap();
        assert_eq!(&back, m);
        rest = &rest[used..];
    }
    assert!(rest.is_empty());
    let frame = encode_frame(&ServiceMessage::Shutdown);
    assert_eq!(
        decode_frame::<ServiceMessage>(&frame[..frame.len() - 1]).unwrap_err(),
        FrameError::Incomplete
    );
}

#[test]
fn pipe_names() {
    assert_eq!(engine_pipe_name(7), r"\\.\pipe\mouser-svc-7");
    assert_eq!(engine_pipe_name(0), r"\\.\pipe\mouser-svc-0");
    assert_eq!(SERVICE_CONTROL_PIPE, r"\\.\pipe\mouser-service");
}
