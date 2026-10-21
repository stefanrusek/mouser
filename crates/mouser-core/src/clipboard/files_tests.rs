//! Copied files (Story 6 scenario 2): a copy sends only a manifest; pasting
//! fetches the bytes through a transfer and writes local copies.

use std::path::{Path, PathBuf};

use mouser_platform::{ClipRep, LocalInput, Platform};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{
    ClipboardKind, ClipboardPayload, ControlMessage, TransferId, TransferItem,
};
use mouser_testkit::VirtualPlatform;

use super::files::{Inbox, manifest};
use super::watch::offer;
use super::{Clip, Command, Out};
use crate::crossing::modifiers::LEFT_CTRL;
use crate::testing::*;

const V: u32 = 0x0007_0019;

fn item(path: &str, size: u64, is_dir: bool) -> TransferItem {
    TransferItem {
        relative_path: path.into(),
        size,
        is_dir,
    }
}

/// `docs/a.txt`, `docs/sub/b.bin` and `notes.txt` under a fresh directory.
fn copied() -> (tempfile::TempDir, Vec<PathBuf>) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("docs/sub")).unwrap();
    std::fs::write(root.join("docs/a.txt"), b"abc").unwrap();
    std::fs::write(root.join("docs/sub/b.bin"), [7u8; 10]).unwrap();
    std::fs::write(root.join("notes.txt"), b"notes").unwrap();
    let paths = vec![root.join("docs"), root.join("notes.txt")];
    (dir, paths)
}

fn items_of(files: &[(TransferItem, PathBuf)]) -> Vec<TransferItem> {
    files.iter().map(|(item, _)| item.clone()).collect()
}

#[test]
fn a_manifest_lists_names_sizes_and_relative_paths_of_files_and_folders() {
    let (_dir, paths) = copied();
    let files = manifest(&paths);
    assert_eq!(
        items_of(&files),
        [
            item("docs", 0, true),
            item("docs/a.txt", 3, false),
            item("docs/sub", 0, true),
            item("docs/sub/b.bin", 10, false),
            item("notes.txt", 5, false),
        ]
    );
    assert_eq!(files[1].1, paths[0].join("a.txt"));
}

#[test]
fn a_file_copy_offers_only_the_manifest() {
    let (_dir, paths) = copied();
    let (message, served) = offer(3, &[ClipRep::Files(paths)]);
    let ControlMessage::ClipboardOffer {
        representations,
        file_manifest,
        ..
    } = message
    else {
        panic!("an offer");
    };
    assert_eq!(representations.len(), 1);
    assert_eq!(representations[0].kind, ClipboardKind::Files);
    assert_eq!(
        representations[0].payload,
        ClipboardPayload::Reference { size: 18 }
    );
    assert_eq!(file_manifest, items_of(&served.files));
    assert_eq!(file_manifest.len(), 5);
}

fn transfer() -> TransferId {
    TransferId([4; 16])
}

#[test]
fn expected_transfers_refuse_paths_that_leave_their_directory() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = Inbox::default();
    for bad in ["../x", "/etc/passwd", "a/../../x"] {
        let error = inbox
            .expect(transfer(), dir.path(), &[item(bad, 1, false)])
            .unwrap_err();
        assert!(error.contains("outside"), "{bad}: {error}");
    }
}

#[test]
fn a_transfer_of_folders_only_is_complete_as_soon_as_they_are_made() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = Inbox::default();
    let done = inbox
        .expect(
            transfer(),
            dir.path(),
            &[item("empty", 0, true), item("empty/inner", 0, true)],
        )
        .unwrap();
    assert_eq!(done, Some(vec![dir.path().join("empty")]));
    assert!(dir.path().join("empty/inner").is_dir());
}

#[test]
fn a_transfer_with_files_waits_for_their_streams() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = Inbox::default();
    let done = inbox
        .expect(transfer(), dir.path(), &[item("f", 1, false)])
        .unwrap();
    assert_eq!(done, None);
}

fn rig() -> (
    VirtualPlatform,
    Box<dyn mouser_platform::Clipboard>,
    Clip,
    tempfile::TempDir,
) {
    let platform = VirtualPlatform::new();
    let mut clip = platform.clipboard().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = Clip::new(&mut *clip, dir.path().to_path_buf(), Inbox::default());
    (platform, clip, state, dir)
}

fn from() -> ComputerId {
    ComputerId([1; 16])
}

#[test]
fn a_transfer_nobody_asked_for_is_a_protocol_violation() {
    let (_platform, mut clip, mut state, _dir) = rig();
    let offered = Command::Message(
        from(),
        ControlMessage::TransferOffer {
            transfer_id: transfer(),
            items: vec![item("f", 1, false)],
            destination_dir: String::new(),
        },
    );
    let error = state.handle(&mut *clip, offered).unwrap_err();
    assert!(error.contains("TransferOffer"), "{error}");
    let accepted = Command::Message(
        from(),
        ControlMessage::TransferAccept {
            transfer_id: transfer(),
        },
    );
    let error = state.handle(&mut *clip, accepted).unwrap_err();
    assert!(error.contains("TransferAccept"), "{error}");
}

#[test]
fn asking_for_files_makes_the_origin_offer_a_transfer_and_streams_follow_the_accept() {
    let (platform, mut clip, mut state, _dir) = rig();
    let (_src, paths) = copied();
    platform.set_local_clipboard(vec![ClipRep::Files(paths)]);
    state.poll(&mut *clip);
    let fetch = Command::Message(
        from(),
        ControlMessage::ClipboardFetch {
            sequence: 1,
            kind: ClipboardKind::Files,
        },
    );
    let outs = state.handle(&mut *clip, fetch).unwrap();
    let [
        Out::To(
            to,
            ControlMessage::TransferOffer {
                transfer_id, items, ..
            },
        ),
    ] = &outs[..]
    else {
        panic!("one transfer offer: {outs:?}");
    };
    assert_eq!(*to, from());
    assert_eq!(items.len(), 5);
    // Only the computer that asked may accept it.
    let accept = |peer| {
        Command::Message(
            peer,
            ControlMessage::TransferAccept {
                transfer_id: *transfer_id,
            },
        )
    };
    let error = state
        .handle(&mut *clip, accept(ComputerId([2; 16])))
        .unwrap_err();
    assert!(error.contains("TransferAccept"), "{error}");
    let outs = state.handle(&mut *clip, accept(from())).unwrap();
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
    assert_eq!((*to, *transfer), (from(), *transfer_id));
    assert_eq!(items_of(files), *items);
    // Done is the receiver's courtesy; the origin has nothing left to do.
    let done = Command::Message(
        from(),
        ControlMessage::TransferDone {
            transfer_id: *transfer_id,
        },
    );
    assert_eq!(state.handle(&mut *clip, done).unwrap(), Vec::<Out>::new());
}

fn read_all(dir: &Path, rel: &str) -> Vec<u8> {
    std::fs::read(dir.join(rel)).unwrap()
}

#[tokio::test]
async fn pasting_copied_files_on_another_computer_writes_local_copies() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (src, paths) = copied();
    a.platform.set_local_clipboard(vec![ClipRep::Files(paths)]);
    // The copy sent a manifest only; Beta has no files yet.
    settle().await;
    assert_eq!(b.platform.clipboard_contents(), Vec::<ClipRep>::new());
    for (usage, down) in [(LEFT_CTRL, true), (V, true), (V, false), (LEFT_CTRL, false)] {
        b.platform.send_input(LocalInput::Key { usage, down });
    }
    eventually("local copies on Beta", || async {
        matches!(&b.platform.clipboard_contents()[..], [ClipRep::Files(_)])
    })
    .await;
    let [ClipRep::Files(pasted)] = &b.platform.clipboard_contents()[..] else {
        panic!("files");
    };
    assert_eq!(pasted.len(), 2);
    assert!(pasted.iter().all(|p| !p.starts_with(src.path())));
    let (docs, notes) = (&pasted[0], &pasted[1]);
    assert_eq!(docs.file_name().unwrap(), "docs");
    assert_eq!(notes.file_name().unwrap(), "notes.txt");
    assert_eq!(read_all(docs, "a.txt"), b"abc");
    assert_eq!(read_all(docs, "sub/b.bin"), [7u8; 10]);
    assert_eq!(std::fs::read(notes).unwrap(), b"notes");
    // The originals are untouched and Alpha's clipboard did not change.
    assert_eq!(a.platform.clipboard().unwrap().change_count(), 1);
}

#[test]
fn files_that_arrive_when_nothing_is_being_fetched_are_ignored() {
    let (platform, mut clip, mut state, _dir) = rig();
    let outs = state
        .handle(&mut *clip, Command::FilesArrived(vec!["/tmp/x".into()]))
        .unwrap();
    assert_eq!(outs, Vec::<Out>::new());
    assert_eq!(platform.clipboard_contents(), Vec::<ClipRep>::new());
}

#[test]
fn a_transfer_of_folders_only_is_done_the_moment_it_is_offered() {
    let (platform, mut clip, mut state, dir) = rig();
    let message = |message| Command::Message(from(), message);
    state
        .handle(
            &mut *clip,
            message(ControlMessage::ClipboardOffer {
                sequence: 1,
                representations: vec![mouser_proto::wire::ClipboardRepresentation {
                    kind: ClipboardKind::Files,
                    payload: ClipboardPayload::Reference { size: 0 },
                }],
                file_manifest: vec![item("empty", 0, true)],
            }),
        )
        .unwrap();
    state.handle(&mut *clip, Command::Paste).unwrap();
    let outs = state
        .handle(
            &mut *clip,
            message(ControlMessage::TransferOffer {
                transfer_id: transfer(),
                items: vec![item("empty", 0, true)],
                destination_dir: String::new(),
            }),
        )
        .unwrap();
    assert_eq!(
        outs,
        [
            Out::To(
                from(),
                ControlMessage::TransferAccept {
                    transfer_id: transfer()
                }
            ),
            Out::To(
                from(),
                ControlMessage::TransferDone {
                    transfer_id: transfer()
                }
            ),
        ]
    );
    let held = dir
        .path()
        .join("04040404040404040404040404040404")
        .join("empty");
    assert_eq!(
        platform.clipboard_contents(),
        [ClipRep::Files(vec![held.clone()])]
    );
    assert!(held.is_dir());
}
