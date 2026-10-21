use std::collections::BTreeMap;

use mouser_proto::control::{FixAction, Problem, ProblemKind};
use mouser_proto::ids::{ComputerId, DeskId, DevicePublicKey};
use mouser_proto::model::*;
use mouser_proto::wire::*;

fn did() -> DisplayId {
    DisplayId {
        computer: ComputerId([1; 16]),
        key: "d0".into(),
    }
}

fn mods() -> Modifiers {
    Modifiers {
        shift: true,
        ctrl: false,
        alt: true,
        meta: false,
    }
}

fn item() -> TransferItem {
    TransferItem {
        relative_path: "a/b.txt".into(),
        size: 42,
        is_dir: false,
    }
}

fn all_messages() -> Vec<ControlMessage> {
    let tid = TransferId([8; 16]);
    let mut permissions = BTreeMap::new();
    permissions.insert(PermissionKind::ScreenRecording, PermissionState::Missing);
    vec![
        ControlMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            mouser_version: "0.1.0".into(),
            computer_id: ComputerId([1; 16]),
            display_name: "mac".into(),
            os: Os::Macos,
            desk_id: Some(DeskId([2; 16])),
        },
        ControlMessage::Hello {
            protocol_version: PROTOCOL_VERSION,
            mouser_version: "0.1.0".into(),
            computer_id: ComputerId([1; 16]),
            display_name: "mac".into(),
            os: Os::Linux,
            desk_id: None,
        },
        ControlMessage::JoinRequest {
            desk_id: DeskId([2; 16]),
            device_public_key: DevicePublicKey([3; 32]),
            display_name: "pc".into(),
            os: Os::Windows,
        },
        ControlMessage::JoinDecision {
            approved: true,
            sealed: Some(vec![1, 2, 3]),
        },
        ControlMessage::JoinDecision {
            approved: false,
            sealed: None,
        },
        ControlMessage::DeskAuth { hmac: [0xcd; 32] },
        ControlMessage::DeskState(DeskState {
            epoch: 3,
            author: ComputerId([5; 16]),
            members: vec![],
            layout: Layout::default(),
            allowlist: vec!["10.0.0.0/8".into()],
        }),
        ControlMessage::Leave {
            computer_id: ComputerId([4; 16]),
        },
        ControlMessage::Remove {
            computer_id: ComputerId([4; 16]),
        },
        ControlMessage::Displays {
            displays: vec![Display {
                display_id: did(),
                name: "d".into(),
                pixel_size: (1920, 1080),
                logical_size: (1920.0, 1080.0),
                scale: 1.0,
                physical_size_mm: None,
                origin_local: (0, 0),
                included: true,
            }],
        },
        ControlMessage::Enter {
            display_id: did(),
            x: 1.5,
            y: -2.5,
            seq_base: 100,
            modifiers_down: mods(),
        },
        ControlMessage::LeaveControl {
            edge: Side::Left,
            position: 321.0,
        },
        ControlMessage::Key {
            seq: 1,
            usage: 0x0007_0004,
            down: true,
            modifiers: mods(),
        },
        ControlMessage::Key {
            seq: 5,
            usage: 0x000C_00E9,
            down: false,
            modifiers: Modifiers::default(),
        },
        ControlMessage::Button {
            seq: 2,
            button: 5,
            down: false,
        },
        ControlMessage::Scroll {
            seq: 3,
            dx: -120,
            dy: 240,
        },
        ControlMessage::ReleaseAll { seq: 4 },
        ControlMessage::Panic,
        ControlMessage::ClipboardOffer {
            sequence: 9,
            representations: vec![
                ClipboardRepresentation {
                    kind: ClipboardKind::TextPlain,
                    payload: ClipboardPayload::Inline(b"hi".to_vec()),
                },
                ClipboardRepresentation {
                    kind: ClipboardKind::ImagePng,
                    payload: ClipboardPayload::Reference { size: 5_000_000 },
                },
                ClipboardRepresentation {
                    kind: ClipboardKind::TextHtml,
                    payload: ClipboardPayload::Inline(vec![]),
                },
                ClipboardRepresentation {
                    kind: ClipboardKind::TextRtf,
                    payload: ClipboardPayload::Inline(vec![]),
                },
                ClipboardRepresentation {
                    kind: ClipboardKind::Files,
                    payload: ClipboardPayload::Inline(vec![]),
                },
            ],
            file_manifest: vec![item()],
        },
        ControlMessage::ClipboardFetch {
            sequence: 9,
            kind: ClipboardKind::ImagePng,
        },
        ControlMessage::ClipboardData {
            sequence: 9,
            kind: ClipboardKind::ImagePng,
            bytes: vec![0; 10],
        },
        ControlMessage::TransferOffer {
            transfer_id: tid,
            items: vec![item()],
            destination_dir: "/home/u/in".into(),
        },
        ControlMessage::TransferAccept { transfer_id: tid },
        ControlMessage::TransferReject {
            transfer_id: tid,
            reason: "no_space".into(),
        },
        ControlMessage::TransferConflict {
            transfer_id: tid,
            item: "a/b.txt".into(),
        },
        ControlMessage::TransferResolve {
            transfer_id: tid,
            overwrite: true,
        },
        ControlMessage::TransferDone { transfer_id: tid },
        ControlMessage::QuickJumpShow {
            letters: vec![(did(), 'A')],
        },
        ControlMessage::QuickJumpHide,
        ControlMessage::ReleaseAvailable {
            manifest_json: r#"{"version":"1.0.0","platforms":{}}"#.into(),
        },
        ControlMessage::ReleaseFetch {
            version: "1.0.0".into(),
            asset: "darwin-aarch64".into(),
        },
        ControlMessage::Status(PeerStatus {
            permissions,
            limitations: vec!["x".into()],
            secure_input_holder: Some(SecureInputHolder {
                pid: 77,
                app_name: "Terminal".into(),
            }),
            problems: vec![Problem {
                problem_id: "k:s".into(),
                kind: ProblemKind::SecureInput,
                computer: ComputerId([1; 16]),
                message: "m".into(),
                fix: Some(FixAction::ApproveJoin),
                raised_at: "2026-05-01T12:00:00Z".into(),
                cleared_at: Some("2026-05-01T12:01:00Z".into()),
            }],
        }),
        ControlMessage::SecureAttention,
    ]
}

#[test]
fn every_control_message_roundtrips_through_framing() {
    let msgs = all_messages();
    assert_eq!(msgs.len(), 33);
    let mut stream = Vec::new();
    for m in &msgs {
        let frame = encode_frame(m);
        let len = u32::from_be_bytes(frame[..4].try_into().unwrap()) as usize;
        assert_eq!(len, frame.len() - 4);
        let (back, used) = decode_frame::<ControlMessage>(&frame).unwrap();
        assert_eq!((&back, used), (m, frame.len()));
        stream.extend(frame);
    }
    // Several frames back to back decode in order.
    let mut rest = &stream[..];
    for m in &msgs {
        let (back, used) = decode_frame::<ControlMessage>(rest).unwrap();
        assert_eq!(&back, m);
        rest = &rest[used..];
    }
    assert!(rest.is_empty());
}

#[test]
fn motion_datagram_roundtrips_and_is_small() {
    let d = MotionDatagram {
        seq: u64::MAX,
        display_id_hash: u32::MAX,
        x: 1919.5,
        y: 1079.25,
    };
    let frame = encode_frame(&d);
    assert!(frame.len() < 1100, "{}", frame.len());
    assert_eq!(
        decode_frame::<MotionDatagram>(&frame).unwrap(),
        (d, frame.len())
    );
}

#[test]
fn truncated_frames_are_incomplete_and_garbage_is_malformed() {
    let frame = encode_frame(&ControlMessage::Panic);
    for cut in 0..frame.len() {
        assert_eq!(
            decode_frame::<ControlMessage>(&frame[..cut]).unwrap_err(),
            FrameError::Incomplete,
            "cut {cut}"
        );
    }
    // Valid length prefix, body is an unknown enum variant.
    let bad = [0, 0, 0, 1, 0xff];
    let err = decode_frame::<ControlMessage>(&bad).unwrap_err();
    assert_eq!(err, FrameError::Malformed);
    assert_eq!(err.to_string(), "malformed frame");
    assert_eq!(FrameError::Incomplete.to_string(), "incomplete frame");
}

#[test]
fn protocol_version_and_compatibility() {
    assert_eq!(PROTOCOL_VERSION, 1);
    assert!(is_compatible(1, 1));
    assert!(!is_compatible(1, 2));
    assert!(!is_compatible(2, 1));
}

#[test]
fn display_id_hash_is_stable_and_discriminates() {
    let a = did();
    assert_eq!(display_id_hash(&a), 0x32bc_ce13);
    assert_eq!(display_id_hash(&a), display_id_hash(&a.clone()));
    let other_key = DisplayId {
        key: "d1".into(),
        ..a.clone()
    };
    let other_computer = DisplayId {
        computer: ComputerId([2; 16]),
        ..a.clone()
    };
    assert_ne!(display_id_hash(&a), display_id_hash(&other_key));
    assert_ne!(display_id_hash(&a), display_id_hash(&other_computer));
}
