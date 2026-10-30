//! contracts/service-ipc.md, producer side: the frames the service writes
//! and the frames it reads round-trip against `mouser_proto::service_ipc`,
//! framed as `mouser_proto::wire` frames are.

use mouser_proto::service_ipc::{EngineMessage, SasResult, ServiceMessage, StopReason};
use mouser_proto::wire::{decode_frame, encode_frame};

/// The length of `frame`, which is a 4-byte big-endian length and then that
/// many bytes of body.
fn framed(frame: &[u8]) -> usize {
    let len = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
    assert_eq!(frame.len(), 4 + len);
    frame.len()
}

fn service(msg: ServiceMessage) {
    let frame = encode_frame(&msg);
    assert_eq!(decode_frame(&frame), Ok((msg, framed(&frame))));
}

fn engine(msg: EngineMessage) {
    let frame = encode_frame(&msg);
    assert_eq!(decode_frame(&frame), Ok((msg, framed(&frame))));
}

#[test]
fn the_services_frames_round_trip() {
    for result in [
        SasResult::Sent,
        SasResult::PolicyBlocked,
        SasResult::Failed("access is denied".into()),
    ] {
        service(ServiceMessage::SasResult(result));
    }
    service(ServiceMessage::Shutdown);
}

#[test]
fn the_engines_frames_round_trip() {
    engine(EngineMessage::Hello {
        version: "0.3.0".into(),
    });
    engine(EngineMessage::SendSas);
    for reason in [StopReason::Quit, StopReason::Fault] {
        engine(EngineMessage::Stopping { reason });
    }
}
