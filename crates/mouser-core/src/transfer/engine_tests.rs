//! The transfer engine (Story 7 scenario 1): states `Pending → Checking →
//! Copying → Completed`, one stream per item, copies in the drop folder with
//! their relative paths.

use std::path::{Path, PathBuf};
use std::time::Instant;

use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ControlMessage, TransferId, TransferItem};

use super::drag::DRAG_DESTINATION;
use super::engine::{Command, Decision, Ids, Out, State, Transfers};

pub(super) fn peer() -> ComputerId {
    ComputerId([1; 16])
}

pub(super) fn stranger() -> ComputerId {
    ComputerId([2; 16])
}

pub(super) fn id() -> TransferId {
    TransferId([9; 16])
}

pub(super) fn item(path: &str, size: u64, is_dir: bool) -> TransferItem {
    TransferItem {
        relative_path: path.into(),
        size,
        is_dir,
    }
}

/// An engine for a computer with `free` bytes to spare.
pub(super) fn engine(free: u64) -> (Transfers, Ids) {
    let ids = Ids::default();
    (Transfers::new(ids.clone(), move |_: &Path| free), ids)
}

pub(super) fn message(from: ComputerId, message: ControlMessage) -> Command {
    Command::Message(from, message)
}

pub(super) fn offer(items: Vec<TransferItem>) -> Command {
    message(
        peer(),
        ControlMessage::TransferOffer {
            transfer_id: id(),
            items,
            destination_dir: DRAG_DESTINATION.into(),
        },
    )
}

/// `docs/a.txt`, `docs/sub/b.bin` and `notes.txt` under a fresh directory.
pub(super) fn dragged() -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("docs/sub")).unwrap();
    std::fs::write(root.join("docs/a.txt"), b"abc").unwrap();
    std::fs::write(root.join("docs/sub/b.bin"), [7u8; 10]).unwrap();
    std::fs::write(root.join("notes.txt"), b"notes").unwrap();
    let paths = vec![root.join("docs"), root.join("notes.txt")];
    (dir, paths)
}

pub(super) fn manifest_items() -> Vec<TransferItem> {
    vec![
        item("docs", 0, true),
        item("docs/a.txt", 3, false),
        item("docs/sub", 0, true),
        item("docs/sub/b.bin", 10, false),
        item("notes.txt", 5, false),
    ]
}

pub(super) fn run(t: &mut Transfers, command: Command) -> Vec<Out> {
    t.handle(command, Instant::now()).unwrap()
}

pub(super) fn fail(t: &mut Transfers, command: Command) -> String {
    t.handle(command, Instant::now()).unwrap_err()
}

#[test]
fn dragging_across_makes_an_offer_of_the_manifest_to_the_computer_crossed_to() {
    let (mut t, ids) = engine(0);
    let (_dir, paths) = dragged();
    let outs = run(&mut t, Command::Drag { to: peer(), paths });
    let [
        Out::To(
            to,
            ControlMessage::TransferOffer {
                transfer_id,
                items,
                destination_dir,
            },
        ),
    ] = &outs[..]
    else {
        panic!("one offer: {outs:?}");
    };
    assert_eq!(*to, peer());
    assert_eq!(*items, manifest_items());
    assert_eq!(destination_dir, DRAG_DESTINATION);
    assert_eq!(t.state_of(transfer_id), Some(State::Pending));
    assert!(ids.contains(transfer_id));
}

#[test]
fn the_source_streams_every_file_once_the_destination_accepts_and_is_done_at_the_done() {
    let (mut t, ids) = engine(0);
    let (_dir, paths) = dragged();
    let outs = run(&mut t, Command::Drag { to: peer(), paths });
    let [Out::To(_, ControlMessage::TransferOffer { transfer_id, .. })] = &outs[..] else {
        panic!("an offer");
    };
    let accept = |from| {
        message(
            from,
            ControlMessage::TransferAccept {
                transfer_id: *transfer_id,
            },
        )
    };
    assert!(fail(&mut t, accept(stranger())).contains("TransferAccept"));
    let outs = run(&mut t, accept(peer()));
    let [
        Out::Stream {
            to,
            transfer,
            files,
        },
    ] = &outs[..]
    else {
        panic!("one stream order: {outs:?}");
    };
    assert_eq!((*to, *transfer), (peer(), *transfer_id));
    // One stream per file (the sender skips folders); the order is the offer's.
    let items: Vec<_> = files.iter().map(|(item, _)| item.clone()).collect();
    assert_eq!(items, manifest_items());
    assert_eq!(t.state_of(transfer_id), Some(State::Copying));
    let done = |from| {
        message(
            from,
            ControlMessage::TransferDone {
                transfer_id: *transfer_id,
            },
        )
    };
    assert!(fail(&mut t, done(stranger())).contains("TransferDone"));
    assert_eq!(run(&mut t, done(peer())), Vec::<Out>::new());
    assert_eq!(t.state_of(transfer_id), None);
    assert!(!ids.contains(transfer_id));
}

#[test]
fn an_offer_makes_the_destination_start_a_drag_of_the_top_level_names() {
    let (mut t, ids) = engine(1 << 20);
    let outs = run(&mut t, offer(manifest_items()));
    assert_eq!(
        outs,
        [Out::Drag {
            id: id(),
            names: vec!["docs".into(), "notes.txt".into()]
        }]
    );
    assert_eq!(t.state_of(&id()), Some(State::Pending));
    assert!(ids.contains(&id()));
}

#[test]
fn an_offer_of_paths_that_leave_the_drop_folder_is_refused() {
    let (mut t, _) = engine(1 << 20);
    for bad in ["../x", "/etc/passwd", "a/../../x"] {
        let error = fail(&mut t, offer(vec![item(bad, 1, false)]));
        assert!(error.contains("outside"), "{bad}: {error}");
    }
}

#[test]
fn copies_appear_in_the_drop_folder_with_their_relative_paths() {
    let (mut t, ids) = engine(1 << 20);
    run(&mut t, offer(manifest_items()));
    let drop = tempfile::tempdir().unwrap();
    let outs = run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    // The folders exist at once; accepted, so the source streams.
    assert!(drop.path().join("docs/sub").is_dir());
    assert!(matches!(
        &outs[..],
        [
            Out::To(to, ControlMessage::TransferAccept { transfer_id }),
            Out::Notify(_)
        ] if *to == peer() && *transfer_id == id()
    ));
    assert_eq!(t.state_of(&id()), Some(State::Copying));
    // The streams arrive in the offer's order; each is told where to write.
    for (index, last) in [(1, false), (3, false), (4, true)] {
        let outs = run(&mut t, Command::ItemStart { id: id(), index });
        let name = manifest_items()[index].relative_path.clone();
        assert_eq!(
            outs,
            [Out::Decide {
                id: id(),
                index,
                decision: Decision::Write(drop.path().join(name))
            }]
        );
        let outs = run(&mut t, Command::ItemDone { id: id() });
        assert_eq!(
            matches!(
                &outs[..],
                [
                    Out::To(_, ControlMessage::TransferDone { .. }),
                    Out::Notify(_)
                ]
            ),
            last
        );
    }
    assert_eq!(t.state_of(&id()), Some(State::Completed));
    assert!(!ids.contains(&id()));
}

#[test]
fn a_transfer_of_folders_only_is_completed_the_moment_it_is_accepted() {
    let (mut t, ids) = engine(0);
    run(&mut t, offer(vec![item("empty", 0, true)]));
    let drop = tempfile::tempdir().unwrap();
    let outs = run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    assert!(matches!(
        &outs[..],
        [
            Out::To(_, ControlMessage::TransferAccept { .. }),
            Out::Notify(_),
            Out::To(_, ControlMessage::TransferDone { .. }),
            Out::Notify(_)
        ]
    ));
    assert!(drop.path().join("empty").is_dir());
    assert_eq!(t.state_of(&id()), Some(State::Completed));
    assert!(!ids.contains(&id()));
}

#[test]
fn a_cancelled_drag_ends_quietly_and_tells_the_source_it_is_done() {
    let (mut t, ids) = engine(1 << 20);
    run(&mut t, offer(manifest_items()));
    assert!(ids.contains(&id()));
    let outs = run(&mut t, Command::Cancelled { id: id() });
    assert_eq!(
        outs,
        [Out::To(
            peer(),
            ControlMessage::TransferDone { transfer_id: id() }
        )]
    );
    // Forgotten: no state, no id, nothing late is of interest.
    assert_eq!(t.state_of(&id()), None);
    assert!(!ids.contains(&id()));
    // A cancel for what nobody offered is a protocol violation.
    assert!(
        t.handle(Command::Cancelled { id: id() }, Instant::now())
            .is_err()
    );
}

#[test]
fn what_nobody_offered_is_a_protocol_violation_or_ignored_when_it_is_late() {
    let (mut t, _) = engine(0);
    let dropped = Command::Dropped {
        id: id(),
        dir: PathBuf::new(),
    };
    assert!(fail(&mut t, dropped).contains("nobody offered"));
    assert!(fail(&mut t, Command::ItemStart { id: id(), index: 0 }).contains("nobody offered"));
    // Bytes still on the way when the transfer ended are of no interest.
    assert!(run(&mut t, Command::Chunk { id: id(), bytes: 1 }).is_empty());
    assert!(run(&mut t, Command::ItemDone { id: id() }).is_empty());
    assert!(fail(&mut t, message(peer(), ControlMessage::SecureAttention)).contains("unexpected"));
}

#[test]
fn a_computer_that_leaves_takes_its_transfers_with_it() {
    let (mut t, ids) = engine(1 << 20);
    let (_dir, paths) = dragged();
    run(&mut t, Command::Drag { to: peer(), paths });
    run(&mut t, offer(manifest_items()));
    run(
        &mut t,
        Command::Drag {
            to: stranger(),
            paths: vec![],
        },
    );
    // Both of its transfers fail (see interrupt_tests); the other stays.
    let outs = run(&mut t, Command::PeerGone(peer(), "Alpha".into()));
    assert_eq!(outs.len(), 3, "{outs:?}");
    assert_eq!(t.state_of(&id()), None);
    // Only the transfer with the other computer is left.
    assert_eq!(t.outbound.len(), 1);
    assert!(t.inbound.is_empty());
    assert!(ids.contains(t.outbound.keys().next().unwrap()));
    assert!(!ids.contains(&id()));
}
