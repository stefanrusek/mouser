//! The free-space check (Story 7 scenario 4): the total size is compared
//! with the free space before any byte is written.

use std::time::Instant;

use mouser_proto::wire::ControlMessage;

use super::engine::{Command, Out, State};
use super::engine_tests::*;
use super::space::{NO_SPACE, NO_SPACE_TEXT, fits, free_space};

#[test]
fn a_transfer_fits_when_it_is_no_bigger_than_the_free_space() {
    assert!(fits(10, 10));
    assert!(fits(0, 0));
    assert!(!fits(11, 10));
}

#[test]
fn the_free_space_of_a_real_folder_is_measured() {
    let dir = tempfile::tempdir().unwrap();
    assert!(free_space(dir.path()) > 0);
}

#[test]
fn too_little_room_rejects_the_transfer_tells_the_user_and_writes_nothing() {
    // 18 bytes of files (3 + 10 + 5) and 17 bytes of room.
    let (mut t, ids) = engine(17);
    run(&mut t, offer(manifest_items()));
    let drop = tempfile::tempdir().unwrap();
    let outs = run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    let [
        Out::To(
            to,
            ControlMessage::TransferReject {
                transfer_id,
                reason,
            },
        ),
        Out::Notify(note),
    ] = &outs[..]
    else {
        panic!("a rejection and a notification: {outs:?}");
    };
    assert_eq!(
        (*to, *transfer_id, reason.as_str()),
        (peer(), id(), NO_SPACE)
    );
    assert_eq!(note["state"], "rejected");
    assert_eq!(note["message"], NO_SPACE_TEXT);
    assert_eq!(note["reason"], NO_SPACE);
    assert_eq!(t.state_of(&id()), Some(State::Rejected));
    assert!(!ids.contains(&id()));
    // Not even the folders were made.
    assert_eq!(std::fs::read_dir(drop.path()).unwrap().count(), 0);
}

#[test]
fn exactly_enough_room_is_enough() {
    let (mut t, _) = engine(18);
    run(&mut t, offer(manifest_items()));
    let drop = tempfile::tempdir().unwrap();
    let outs = run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    assert!(matches!(
        &outs[0],
        Out::To(_, ControlMessage::TransferAccept { .. })
    ));
}

#[test]
fn the_space_is_measured_where_the_files_were_dropped() {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let recorder = seen.clone();
    let ids = super::engine::Ids::default();
    let mut t = super::engine::Transfers::new(ids, move |dir: &std::path::Path| {
        recorder.lock().unwrap().push(dir.to_path_buf());
        u64::MAX
    });
    run(&mut t, offer(vec![item("f", 1, false)]));
    let drop = tempfile::tempdir().unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    assert_eq!(*seen.lock().unwrap(), [drop.path().to_path_buf()]);
}

#[test]
fn the_source_is_told_when_the_other_computer_has_no_room() {
    let (mut t, ids) = engine(0);
    let (_dir, paths) = dragged();
    let outs = run(&mut t, Command::Drag { to: peer(), paths });
    let [Out::To(_, ControlMessage::TransferOffer { transfer_id, .. })] = &outs[..] else {
        panic!("an offer");
    };
    let reject = |from| {
        message(
            from,
            ControlMessage::TransferReject {
                transfer_id: *transfer_id,
                reason: NO_SPACE.into(),
            },
        )
    };
    assert!(fail(&mut t, reject(stranger())).contains("TransferReject"));
    let outs = t.handle(reject(peer()), Instant::now()).unwrap();
    let [Out::Notify(note)] = &outs[..] else {
        panic!("one notification: {outs:?}");
    };
    assert_eq!(note["state"], "rejected");
    assert_eq!(note["message"], NO_SPACE_TEXT);
    assert_eq!(note["bytes_total"], 18);
    assert_eq!(t.state_of(transfer_id), None);
    assert!(!ids.contains(transfer_id));
}
