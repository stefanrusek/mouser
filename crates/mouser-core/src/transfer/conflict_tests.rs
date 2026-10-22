//! Name conflicts (Story 7 scenario 3): a taken name pauses that item for
//! the user's answer; `TransferResolve{overwrite}` continues or skips it.

use std::path::PathBuf;
use std::time::Instant;

use mouser_proto::wire::ControlMessage;

use super::engine::{Command, Decision, Out, State, Transfers};
use super::engine_tests::*;

/// Two files offered, `a.txt` already at the destination; the stream of
/// `a.txt` has arrived.
fn conflicted() -> (Transfers, tempfile::TempDir, Vec<Out>) {
    let (mut t, _) = engine(1 << 20);
    run(
        &mut t,
        offer(vec![item("a.txt", 4, false), item("b.txt", 6, false)]),
    );
    let drop = tempfile::tempdir().unwrap();
    std::fs::write(drop.path().join("a.txt"), b"old").unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    let outs = run(&mut t, Command::ItemStart { id: id(), index: 0 });
    (t, drop, outs)
}

#[test]
fn a_taken_name_pauses_the_transfer_and_asks() {
    let (t, _drop, outs) = conflicted();
    let [
        Out::To(to, ControlMessage::TransferConflict { transfer_id, item }),
        Out::Notify(note),
    ] = &outs[..]
    else {
        panic!("a conflict and a notification: {outs:?}");
    };
    assert_eq!((*to, *transfer_id, item.as_str()), (peer(), id(), "a.txt"));
    assert_eq!(note["state"], "awaiting_overwrite_decision");
    assert_eq!(note["item"], "a.txt");
    assert_eq!(note["transfer_id"], "09090909090909090909090909090909");
    assert!(note["message"].as_str().unwrap().contains("a.txt"));
    assert_eq!(t.state_of(&id()), Some(State::AwaitingOverwriteDecision));
}

#[test]
fn the_source_hears_of_the_question_without_acting_on_it() {
    let (mut t, _) = engine(0);
    let conflict = message(
        peer(),
        ControlMessage::TransferConflict {
            transfer_id: id(),
            item: "a.txt".into(),
        },
    );
    assert!(run(&mut t, conflict).is_empty());
}

#[test]
fn overwriting_writes_the_file_and_carries_on() {
    let (mut t, drop, _) = conflicted();
    let outs = run(
        &mut t,
        Command::Resolve {
            transfer_id: "09090909090909090909090909090909".into(),
            overwrite: true,
        },
    );
    let [
        Out::Decide {
            id: got,
            index,
            decision,
        },
        Out::Notify(note),
    ] = &outs[..]
    else {
        panic!("a decision and a notification: {outs:?}");
    };
    assert_eq!((*got, *index), (id(), 0));
    assert_eq!(*decision, Decision::Write(drop.path().join("a.txt")));
    assert_eq!(note["state"], "copying");
    assert_eq!(note["bytes_total"], 10);
    assert_eq!(t.state_of(&id()), Some(State::Copying));
}

#[test]
fn not_overwriting_skips_just_that_item() {
    let (mut t, _drop, _) = conflicted();
    let outs = run(
        &mut t,
        Command::Resolve {
            transfer_id: "09090909090909090909090909090909".into(),
            overwrite: false,
        },
    );
    let [Out::Decide { decision, .. }, Out::Notify(note)] = &outs[..] else {
        panic!("a decision and a notification: {outs:?}");
    };
    assert_eq!(*decision, Decision::Skip);
    // The skipped file is not waited for.
    assert_eq!(note["bytes_total"], 6);
    // The rest goes on, and ends the transfer.
    run(&mut t, Command::ItemDone { id: id() });
    let outs = run(&mut t, Command::ItemStart { id: id(), index: 1 });
    assert!(matches!(
        &outs[..],
        [Out::Decide {
            decision: Decision::Write(_),
            ..
        }]
    ));
    let outs = run(&mut t, Command::ItemDone { id: id() });
    assert!(matches!(
        &outs[0],
        Out::To(_, ControlMessage::TransferDone { .. })
    ));
    assert_eq!(t.state_of(&id()), Some(State::Completed));
}

#[test]
fn the_answer_may_come_from_the_source_as_a_resolve_message() {
    let (mut t, drop, _) = conflicted();
    let resolve = |from| {
        message(
            from,
            ControlMessage::TransferResolve {
                transfer_id: id(),
                overwrite: true,
            },
        )
    };
    assert!(fail(&mut t, resolve(stranger())).contains("TransferResolve"));
    let outs = t.handle(resolve(peer()), Instant::now()).unwrap();
    assert!(matches!(
        &outs[0],
        Out::Decide { decision: Decision::Write(path), .. } if *path == drop.path().join("a.txt")
    ));
}

#[test]
fn an_answer_to_a_question_nobody_asked_is_a_protocol_violation() {
    let (mut t, _) = engine(1 << 20);
    run(&mut t, offer(vec![item("a.txt", 4, false)]));
    let answer = |name: &str| Command::Resolve {
        transfer_id: name.into(),
        overwrite: true,
    };
    assert!(fail(&mut t, answer("09090909090909090909090909090909")).contains("nobody asked"));
    assert!(fail(&mut t, answer("no such transfer")).contains("no such transfer"));
}

#[test]
fn a_free_name_and_an_existing_folder_are_no_conflict() {
    let (mut t, _) = engine(1 << 20);
    run(
        &mut t,
        offer(vec![item("docs", 0, true), item("docs/a.txt", 1, false)]),
    );
    let drop = tempfile::tempdir().unwrap();
    std::fs::create_dir(drop.path().join("docs")).unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    let outs = run(&mut t, Command::ItemStart { id: id(), index: 1 });
    let want: PathBuf = drop.path().join("docs/a.txt");
    assert_eq!(
        outs,
        [Out::Decide {
            id: id(),
            index: 1,
            decision: Decision::Write(want)
        }]
    );
}
