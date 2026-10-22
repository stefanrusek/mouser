//! Interruption (data-model "Transfer"): when the other computer goes
//! offline mid-copy both sides move the transfer to `Failed(peer_offline)`
//! and tell the user, in plain language, how far it got.

use mouser_proto::wire::ControlMessage;
use serde_json::Value;

use super::engine::{Command, Out, State, Transfers};
use super::engine_tests::*;

fn gone(t: &mut Transfers, who: mouser_proto::ids::ComputerId, name: &str) -> Vec<Out> {
    run(t, Command::PeerGone(who, name.into()))
}

fn note(outs: &[Out]) -> &Value {
    outs.iter()
        .find_map(|out| match out {
            Out::Notify(note) => Some(note),
            _ => None,
        })
        .expect("a notification")
}

/// A destination that has accepted the five-item offer (three files) and
/// received the first file; the second one is arriving.
fn receiving() -> (Transfers, super::engine::Ids, tempfile::TempDir) {
    let (mut t, ids) = engine(1 << 20);
    run(&mut t, offer(manifest_items()));
    let drop = tempfile::tempdir().unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    run(&mut t, Command::ItemStart { id: id(), index: 1 });
    run(&mut t, Command::ItemDone { id: id() });
    run(&mut t, Command::ItemStart { id: id(), index: 3 });
    run(&mut t, Command::Chunk { id: id(), bytes: 4 });
    (t, ids, drop)
}

#[test]
fn the_destination_fails_the_copy_and_says_how_many_files_made_it() {
    let (mut t, ids, _drop) = receiving();
    let outs = gone(&mut t, peer(), "Alpha");
    assert_eq!(outs.len(), 2, "{outs:?}");
    assert_eq!(outs[0], Out::Forget { id: id() });
    let note = note(&outs);
    assert_eq!(note["state"], "failed");
    assert_eq!(note["reason"], "peer_offline");
    assert_eq!(note["transfer_id"], "09090909090909090909090909090909");
    assert_eq!(
        note["message"],
        "The copy stopped because Alpha went offline. 1 of 3 files were copied."
    );
    // The bytes that did arrive are still shown.
    assert_eq!(note["bytes_done"], 4);
    assert_eq!(note["bytes_total"], 18);
    assert_eq!(t.state_of(&id()), None);
    assert!(!ids.contains(&id()));
}

#[test]
fn the_source_fails_the_copy_and_says_how_many_files_the_destination_received() {
    let (mut t, ids) = engine(0);
    let (_dir, paths) = dragged();
    let outs = run(&mut t, Command::Drag { to: peer(), paths });
    let [Out::To(_, ControlMessage::TransferOffer { transfer_id, .. })] = &outs[..] else {
        panic!("an offer");
    };
    let transfer_id = *transfer_id;
    run(
        &mut t,
        message(peer(), ControlMessage::TransferAccept { transfer_id }),
    );
    run(&mut t, Command::Sent { id: transfer_id });
    run(&mut t, Command::Sent { id: transfer_id });
    // Another computer's transfer is not ours to count.
    run(&mut t, Command::Sent { id: id() });
    let outs = gone(&mut t, peer(), "Beta");
    assert_eq!(outs.len(), 1, "{outs:?}");
    let note = note(&outs);
    assert_eq!(note["state"], "failed");
    assert_eq!(note["reason"], "peer_offline");
    assert_eq!(
        note["message"],
        "The copy stopped because Beta went offline. 2 of 3 files were copied."
    );
    assert_eq!(note["bytes_total"], 18);
    assert_eq!(t.state_of(&transfer_id), None);
    assert!(!ids.contains(&transfer_id));
}

#[test]
fn a_single_file_is_spoken_of_in_the_singular() {
    let (mut t, _) = engine(0);
    let (_dir, mut paths) = dragged();
    paths.remove(0);
    run(&mut t, Command::Drag { to: peer(), paths });
    let outs = gone(&mut t, peer(), "Beta");
    assert_eq!(
        note(&outs)["message"],
        "The copy stopped because Beta went offline. 0 of 1 file was copied."
    );
}

#[test]
fn a_file_the_user_declined_to_replace_was_not_copied() {
    let (mut t, _) = engine(1 << 20);
    run(&mut t, offer(manifest_items()));
    let drop = tempfile::tempdir().unwrap();
    std::fs::write(drop.path().join("notes.txt"), b"old").unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    run(&mut t, Command::ItemStart { id: id(), index: 1 });
    run(&mut t, Command::ItemDone { id: id() });
    run(&mut t, Command::ItemStart { id: id(), index: 4 });
    run(
        &mut t,
        Command::Resolve {
            transfer_id: "09090909090909090909090909090909".into(),
            overwrite: false,
        },
    );
    run(&mut t, Command::ItemDone { id: id() });
    let outs = gone(&mut t, peer(), "Alpha");
    assert_eq!(
        note(&outs)["message"],
        "The copy stopped because Alpha went offline. 1 of 3 files were copied."
    );
}

#[test]
fn a_file_being_thrown_away_when_the_computer_goes_was_not_copied_either() {
    let (mut t, _) = engine(1 << 20);
    run(&mut t, offer(vec![item("a.txt", 4, false)]));
    let drop = tempfile::tempdir().unwrap();
    std::fs::write(drop.path().join("a.txt"), b"old").unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    run(&mut t, Command::ItemStart { id: id(), index: 0 });
    run(
        &mut t,
        Command::Resolve {
            transfer_id: "09090909090909090909090909090909".into(),
            overwrite: false,
        },
    );
    let outs = gone(&mut t, peer(), "Alpha");
    assert_eq!(
        note(&outs)["message"],
        "The copy stopped because Alpha went offline. 0 of 1 file was copied."
    );
}

#[test]
fn a_question_waiting_for_the_user_is_dropped_with_the_transfer() {
    let (mut t, _) = engine(1 << 20);
    run(&mut t, offer(vec![item("a.txt", 4, false)]));
    let drop = tempfile::tempdir().unwrap();
    std::fs::write(drop.path().join("a.txt"), b"old").unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    run(&mut t, Command::ItemStart { id: id(), index: 0 });
    assert_eq!(t.state_of(&id()), Some(State::AwaitingOverwriteDecision));
    let outs = gone(&mut t, peer(), "Alpha");
    assert_eq!(outs[0], Out::Forget { id: id() });
    assert_eq!(note(&outs)["state"], "failed");
    // The user's late answer finds nothing to answer.
    let late = Command::Resolve {
        transfer_id: "09090909090909090909090909090909".into(),
        overwrite: true,
    };
    assert!(fail(&mut t, late).contains("no transfer"));
    // And the file that was there is untouched.
    assert_eq!(std::fs::read(drop.path().join("a.txt")).unwrap(), b"old");
}

#[test]
fn finished_and_other_transfers_are_not_failed() {
    let (mut t, _) = engine(1 << 20);
    run(&mut t, offer(vec![item("a.txt", 4, false)]));
    let drop = tempfile::tempdir().unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    run(&mut t, Command::ItemStart { id: id(), index: 0 });
    run(&mut t, Command::ItemDone { id: id() });
    assert_eq!(t.state_of(&id()), Some(State::Completed));
    // Done is done: nothing to tell, and it is forgotten with the computer.
    assert!(gone(&mut t, peer(), "Alpha").is_empty());
    assert_eq!(t.state_of(&id()), None);
    // A computer without transfers changes nothing.
    let (mut t, _, _drop) = receiving();
    assert!(gone(&mut t, stranger(), "Gamma").is_empty());
    assert_eq!(t.state_of(&id()), Some(State::Copying));
}

#[test]
fn files_still_arriving_for_a_failed_transfer_are_set_aside() {
    let (mut t, _, drop) = receiving();
    gone(&mut t, peer(), "Alpha");
    // A drop that was under way when the computer went: nothing to accept.
    let dropped = Command::Dropped {
        id: id(),
        dir: drop.path().to_path_buf(),
    };
    assert!(run(&mut t, dropped).is_empty());
    // A stream that was just starting is told to stop waiting.
    let outs = run(&mut t, Command::ItemStart { id: id(), index: 4 });
    assert_eq!(outs, [Out::Forget { id: id() }]);
    // Bytes and ends of files are of no interest.
    assert!(run(&mut t, Command::Chunk { id: id(), bytes: 1 }).is_empty());
    assert!(run(&mut t, Command::ItemDone { id: id() }).is_empty());
    assert_eq!(t.state_of(&id()), None);
}

#[test]
fn a_drag_cancelled_after_the_computer_went_is_of_no_interest() {
    let (mut t, _, _drop) = receiving();
    gone(&mut t, peer(), "Alpha");
    assert!(run(&mut t, Command::Cancelled { id: id() }).is_empty());
}

#[test]
fn only_a_copy_that_is_going_on_can_fail() {
    for (state, going) in [
        (State::Pending, true),
        (State::Checking, true),
        (State::Copying, true),
        (State::AwaitingOverwriteDecision, true),
        (State::Completed, false),
        (State::Rejected, false),
        (State::Failed, false),
    ] {
        assert_eq!(state.in_progress(), going, "{state:?}");
    }
    assert_eq!(State::Failed.name(), "failed");
}

mod when_the_computer_drops_off_the_network {
    use mouser_proto::control::Topic;

    use crate::testing::*;

    /// The next `transfer` notification in `state`.
    async fn until_state(
        notes: &mut tokio::sync::broadcast::Receiver<mouser_proto::control::Notification>,
        state: &str,
    ) -> serde_json::Value {
        loop {
            let note = recv_topic(notes, Topic::Transfer).await;
            if note["state"] == state {
                return note;
            }
        }
    }

    #[tokio::test]
    async fn a_copy_stops_on_both_sides_and_only_the_unfinished_file_is_removed() {
        let lab = Lab::simulated();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a-small.txt"), b"small").unwrap();
        // A hole: it reads as zeros and costs the disk nothing.
        let big = std::fs::File::create(src.path().join("b-big.bin")).unwrap();
        big.set_len(512 << 20).unwrap();
        let drop = tempfile::tempdir().unwrap();
        b.platform.script_drop_target(drop.path().to_path_buf());
        let mut alpha_notes = a.engine.control().notifications();
        let mut beta_notes = b.engine.control().notifications();
        a.platform.script_outgoing_drag(Some(vec![
            src.path().join("a-small.txt"),
            src.path().join("b-big.bin"),
        ]));
        push_right(a);
        // The big file is on its way.
        loop {
            let note = until_state(&mut beta_notes, "copying").await;
            if note["current_item"] == "b-big.bin" {
                break;
            }
        }
        assert!(drop.path().join("b-big.bin").exists());
        b.sim.as_ref().unwrap().set_blackhole(true);
        let beta = until_state(&mut beta_notes, "failed").await;
        assert_eq!(beta["reason"], "peer_offline");
        assert_eq!(
            beta["message"],
            "The copy stopped because Alpha went offline. 1 of 2 files were copied."
        );
        let alpha = until_state(&mut alpha_notes, "failed").await;
        assert_eq!(alpha["reason"], "peer_offline");
        assert_eq!(alpha["transfer_id"], beta["transfer_id"]);
        assert_eq!(
            alpha["message"],
            "The copy stopped because Beta went offline. 1 of 2 files were copied."
        );
        // The finished file stays; the one that was being written goes.
        eventually("the partial file to be deleted", || async {
            !drop.path().join("b-big.bin").exists()
        })
        .await;
        assert_eq!(
            std::fs::read(drop.path().join("a-small.txt")).unwrap(),
            b"small"
        );
        // Nothing is retried: both ends have forgotten the transfer.
        assert!(a.engine.ctx.transfer_ids.is_empty());
        assert!(b.engine.ctx.transfer_ids.is_empty());
        settle().await;
        assert!(!drop.path().join("b-big.bin").exists());
    }

    #[tokio::test]
    async fn a_question_nobody_can_answer_any_more_leaves_the_existing_file_alone() {
        let lab = Lab::simulated();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let src = tempfile::tempdir().unwrap();
        std::fs::write(src.path().join("a.txt"), b"new").unwrap();
        let drop = tempfile::tempdir().unwrap();
        std::fs::write(drop.path().join("a.txt"), b"old").unwrap();
        b.platform.script_drop_target(drop.path().to_path_buf());
        let mut beta_notes = b.engine.control().notifications();
        a.platform
            .script_outgoing_drag(Some(vec![src.path().join("a.txt")]));
        push_right(a);
        until_state(&mut beta_notes, "awaiting_overwrite_decision").await;
        b.sim.as_ref().unwrap().set_blackhole(true);
        let beta = until_state(&mut beta_notes, "failed").await;
        assert_eq!(
            beta["message"],
            "The copy stopped because Alpha went offline. 0 of 1 file was copied."
        );
        eventually("the waiting stream to be let go", || async {
            b.engine.ctx.xfer_waiters.lock().unwrap().is_empty()
        })
        .await;
        assert_eq!(std::fs::read(drop.path().join("a.txt")).unwrap(), b"old");
    }
    #[tokio::test]
    async fn a_file_the_user_declined_to_replace_is_not_a_partial_file_either() {
        let lab = Lab::simulated();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let src = tempfile::tempdir().unwrap();
        let big = std::fs::File::create(src.path().join("big.bin")).unwrap();
        big.set_len(512 << 20).unwrap();
        let drop = tempfile::tempdir().unwrap();
        std::fs::write(drop.path().join("big.bin"), b"old").unwrap();
        b.platform.script_drop_target(drop.path().to_path_buf());
        let mut beta_notes = b.engine.control().notifications();
        a.platform
            .script_outgoing_drag(Some(vec![src.path().join("big.bin")]));
        push_right(a);
        let asked = until_state(&mut beta_notes, "awaiting_overwrite_decision").await;
        b.engine
            .control()
            .resolve_transfer(asked["transfer_id"].as_str().unwrap(), false);
        // The unwanted file is being read and thrown away when the computer goes.
        until_state(&mut beta_notes, "copying").await;
        b.sim.as_ref().unwrap().set_blackhole(true);
        let beta = until_state(&mut beta_notes, "failed").await;
        assert_eq!(
            beta["message"],
            "The copy stopped because Alpha went offline. 0 of 1 file was copied."
        );
        settle().await;
        assert_eq!(std::fs::read(drop.path().join("big.bin")).unwrap(), b"old");
    }
}
