use std::collections::BTreeMap;

use mouser_proto::control::{FixAction, Problem, ProblemKind};
use mouser_proto::ids::{ComputerId, DeskId, DevicePublicKey};
use mouser_proto::model::*;
use mouser_proto::wire::*;
use sha2::{Digest, Sha256};

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
            mode: ComputerMode::Portable,
            problems: vec![Problem {
                problem_id: "k:s".into(),
                app: Some("regedit.exe".into()),
                kind: ProblemKind::SecureInput,
                computer: ComputerId([1; 16]),
                message: "m".into(),
                fix: Some(FixAction::ApproveJoin),
                raised_at: "2026-05-01T12:00:00Z".into(),
                cleared_at: Some("2026-05-01T12:01:00Z".into()),
            }],
        }),
        ControlMessage::SecureAttention,
        ControlMessage::Paused { paused: true },
        ControlMessage::Paused { paused: false },
        ControlMessage::LogsRequest { request: [7; 16] },
        ControlMessage::DeskSettings {
            epoch: 12,
            author: ComputerId([6; 16]),
            landing_highlight: false,
            command_as_control: true,
        },
        ControlMessage::RestartElevated,
    ]
}

#[test]
fn every_control_message_roundtrips_through_framing() {
    let msgs = all_messages();
    assert_eq!(msgs.len(), 38);
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

/// The 0.2.8 variant indexes, in `all_messages()` order, then the three new ones.
const VARIANT_INDEXES: [u8; 38] = [
    0, 0, 1, 2, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
    24, 25, 26, 27, 28, 29, 30, 30, 31, 32, 33,
];

#[test]
fn control_message_variant_indexes_are_a_snapshot() {
    let msgs = all_messages();
    for (m, index) in msgs.iter().zip(VARIANT_INDEXES) {
        let body = &encode_frame(m)[4..];
        assert_eq!(body[0], index, "{m:?}");
    }
    // Paused is the last 0.2.8 variant; the three new ones follow in order.
    assert_eq!(
        encode_frame(&ControlMessage::Paused { paused: true })[4],
        30
    );
    assert_eq!(
        encode_frame(&ControlMessage::LogsRequest { request: [0; 16] })[4],
        31
    );
    assert_eq!(encode_frame(&ControlMessage::RestartElevated)[4..], [33]);
}

#[test]
fn v2_constants() {
    assert_eq!(V2_SINCE, "0.3.0");
    assert_eq!(PROTOCOL_VERSION, 1);
}

#[test]
fn unknown_variant_is_distinct_from_malformed() {
    // Valid length prefix, body names variant 34 (one past RestartElevated).
    let unknown = [0, 0, 0, 3, 34, 1, 2];
    let err = decode_frame::<ControlMessage>(&unknown).unwrap_err();
    assert_eq!(err, FrameError::UnknownVariant(34));
    assert_eq!(err.to_string(), "unknown message variant 34");
    // A multi-byte varint index.
    let big = [0, 0, 0, 2, 0x80, 0x01];
    assert_eq!(
        decode_frame::<ControlMessage>(&big).unwrap_err(),
        FrameError::UnknownVariant(128)
    );
    // Known variant, undecodable body: a bad nested enum (Os) is Malformed.
    let mut hello = encode_frame(&all_messages()[0]);
    hello[32] = 9;
    assert_eq!(
        decode_frame::<ControlMessage>(&hello).unwrap_err(),
        FrameError::Malformed
    );
    // Known variant, a value its type rejects (an unknown fix action).
    let mut status = encode_frame(&all_messages()[31]);
    let at = status
        .windows(b"approve_join".len())
        .position(|w| w == b"approve_join")
        .unwrap();
    status[at + 11] = b'X';
    assert_eq!(
        decode_frame::<ControlMessage>(&status).unwrap_err(),
        FrameError::Malformed
    );
    // Known variant, truncated body.
    let truncated = [0, 0, 0, 2, 28, 0];
    assert_eq!(
        decode_frame::<ControlMessage>(&truncated).unwrap_err(),
        FrameError::Malformed
    );
    assert_ne!(FrameError::UnknownVariant(0), FrameError::Malformed);
}

fn logs_part() -> LogsPart {
    LogsPart {
        card: LogsCard {
            id: ComputerId([1; 16]),
            name: "pc".into(),
            os: Os::Windows,
            os_version: "11 23H2".into(),
            version: "0.3.0".into(),
            mode: ComputerMode::Service,
        },
        config_toml: "schema = 1\n".into(),
        logging: true,
        files: vec![("mouser.log".into(), 10), ("mouser.log.1".into(), 4)],
    }
}

#[test]
fn logs_part_roundtrips_and_stream_id_is_derived() {
    let part = logs_part();
    let frame = encode_frame(&part);
    assert_eq!(
        decode_frame::<LogsPart>(&frame).unwrap(),
        (part, frame.len())
    );
    let request = [9u8; 16];
    let mut hasher = Sha256::new();
    hasher.update(b"logs\n");
    hasher.update(request);
    let digest = hasher.finalize();
    assert_eq!(logs_stream_id(&request), digest[..16]);
    assert_ne!(logs_stream_id(&request), logs_stream_id(&[8; 16]));
}

#[test]
fn computer_mode_names() {
    for (mode, name) in [
        (ComputerMode::Installed, "installed"),
        (ComputerMode::Portable, "portable"),
        (ComputerMode::PortableElevated, "portable_elevated"),
        (ComputerMode::Service, "service"),
        (ComputerMode::Headless, "headless"),
    ] {
        assert_eq!(serde_json::to_value(mode).unwrap(), name);
        let bytes = postcard::to_stdvec(&mode).unwrap();
        assert_eq!(postcard::from_bytes::<ComputerMode>(&bytes).unwrap(), mode);
    }
}

#[test]
fn peer_status_mode_is_last_and_legacy_shape_has_none() {
    let ControlMessage::Status(status) = all_messages()[31].clone() else {
        panic!("index 31 is the Status sample");
    };
    // `mode` is the last field: the encoding ends with the index of Portable.
    let current = encode_frame(&ControlMessage::Status(status.clone()));
    assert_eq!(*current.last().unwrap(), 1);
    assert_eq!(postcard::to_stdvec(&ComputerMode::Portable).unwrap(), [1]);

    // 001 shape: no `mode`, and `Problem` without `app`.
    let legacy = encode_status_v1(&status);
    assert_eq!(
        u32::from_be_bytes(legacy[..4].try_into().unwrap()) as usize,
        legacy.len() - 4
    );
    let (variant, rest) = postcard::take_from_bytes::<u32>(&legacy[4..]).unwrap();
    assert_eq!(variant, 28);
    let v1: PeerStatusV1 = postcard::from_bytes(rest).unwrap();
    assert_eq!(v1, status.to_v1());
    assert_eq!(v1.permissions, status.permissions);
    assert_eq!(v1.problems[0].problem_id, "k:s");
    // Back to the current shape once the peer's mode is known.
    let back = v1.with_mode(ComputerMode::Headless);
    assert_eq!(back.mode, ComputerMode::Headless);
    assert_eq!(back.problems[0].app, None);
    assert_eq!(back.problems[0].message, "m");
}

#[test]
fn decode_status_v1_reads_a_0_2_x_status_frame() {
    let ControlMessage::Status(status) = all_messages()[31].clone() else {
        panic!("index 31 is the Status sample");
    };
    let frame = encode_status_v1(&status);
    let (v1, used) = decode_status_v1(&frame).unwrap();
    assert_eq!(used, frame.len());
    assert_eq!(v1, status.to_v1());
    let current = v1.with_mode(ComputerMode::Headless);
    assert_eq!(current.problems[0].app, None);
    assert_eq!(current.mode, ComputerMode::Headless);

    // Trailing bytes of the next frame are left alone.
    let mut two = frame.clone();
    two.extend(&frame);
    assert_eq!(decode_status_v1(&two).unwrap().1, frame.len());
}

#[test]
fn decode_status_v1_rejects_short_other_and_garbled_frames() {
    let ControlMessage::Status(status) = all_messages()[31].clone() else {
        panic!("index 31 is the Status sample");
    };
    let frame = encode_status_v1(&status);
    assert_eq!(
        decode_status_v1(&frame[..frame.len() - 1]),
        Err(FrameError::Incomplete)
    );
    // A frame for another message is not a status.
    let other = encode_frame(&all_messages()[0]);
    assert_eq!(decode_status_v1(&other), Err(FrameError::Malformed));
    // Status variant index followed by garbage.
    let mut body = vec![28u8, 0xFF, 0xFF];
    let mut garbled = (body.len() as u32).to_be_bytes().to_vec();
    garbled.append(&mut body);
    assert_eq!(decode_status_v1(&garbled), Err(FrameError::Malformed));
}
