//! Drag hand-off (decision D3 item 3): a drag that crosses the screen edge
//! is offered to the computer crossed to, which starts a drag of the files
//! and copies them to where they were dropped.

use mouser_platform::Platform;
use mouser_proto::control::Topic;
use mouser_proto::wire::{ControlMessage, TransferId};
use mouser_testkit::VirtualPlatform;

use super::drag::{DRAG_DESTINATION, is_drag};
use super::engine::Ids;
use super::engine_tests::{dragged, item};
use crate::testing::*;

#[test]
fn offers_say_whether_they_are_drags_and_the_rest_follow_their_transfer() {
    let ids = Ids::default();
    let offer = |destination: &str| ControlMessage::TransferOffer {
        transfer_id: TransferId([1; 16]),
        items: vec![item("f", 1, false)],
        destination_dir: destination.into(),
    };
    assert!(is_drag(&offer(DRAG_DESTINATION), &ids));
    // Pasted files leave the destination to the receiver.
    assert!(!is_drag(&offer(""), &ids));
    let known = TransferId([2; 16]);
    let accept = |id| ControlMessage::TransferAccept { transfer_id: id };
    let done = |id| ControlMessage::TransferDone { transfer_id: id };
    assert!(!is_drag(&accept(known), &ids));
    assert!(!is_drag(&done(known), &ids));
    // Only drags know rejection, conflict and resolution.
    for message in [
        ControlMessage::TransferReject {
            transfer_id: known,
            reason: "no_space".into(),
        },
        ControlMessage::TransferConflict {
            transfer_id: known,
            item: "f".into(),
        },
        ControlMessage::TransferResolve {
            transfer_id: known,
            overwrite: true,
        },
    ] {
        assert!(is_drag(&message, &ids));
    }
}

#[tokio::test]
async fn files_dragged_across_the_edge_are_offered_and_copied_to_the_drop_folder() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (_src, paths) = dragged();
    let drop = tempfile::tempdir().unwrap();
    b.platform.script_drop_target(drop.path().to_path_buf());
    let mut notes = b.engine.control().notifications();
    a.platform.script_outgoing_drag(Some(paths));
    push_right(a);
    // Beta's platform was asked to start a drag of the top-level names.
    eventually("Beta's drag", || async {
        b.platform.incoming_drags() == [vec!["docs".to_string(), "notes.txt".to_string()]]
    })
    .await;
    loop {
        let note = recv_topic(&mut notes, Topic::Transfer).await;
        if note["state"] == "completed" {
            assert_eq!(note["bytes_total"], 18);
            assert_eq!(note["bytes_done"], 18);
            break;
        }
    }
    assert_eq!(
        std::fs::read(drop.path().join("docs/a.txt")).unwrap(),
        b"abc"
    );
    assert_eq!(
        std::fs::read(drop.path().join("docs/sub/b.bin")).unwrap(),
        [7u8; 10]
    );
    assert_eq!(
        std::fs::read(drop.path().join("notes.txt")).unwrap(),
        b"notes"
    );
    // Both ends are done with it.
    eventually("both ends to forget the transfer", || async {
        a.engine.ctx.transfer_ids.is_empty() && b.engine.ctx.transfer_ids.is_empty()
    })
    .await;
}

#[tokio::test]
async fn a_cancelled_drag_ends_on_both_computers_without_a_word() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (_src, paths) = dragged();
    let drop = tempfile::tempdir().unwrap();
    b.platform.script_drop_target(drop.path().to_path_buf());
    b.platform.script_drag_cancelled(true);
    a.platform.script_outgoing_drag(Some(paths));
    push_right(a);
    eventually("Beta's drag", || async {
        !b.platform.incoming_drags().is_empty()
    })
    .await;
    // Beta told Alpha, so both forget the transfer.
    eventually("both ends to forget the transfer", || async {
        a.engine.ctx.transfer_ids.is_empty() && b.engine.ctx.transfer_ids.is_empty()
    })
    .await;
    assert_eq!(std::fs::read_dir(drop.path()).unwrap().count(), 0);
}

#[tokio::test]
async fn nothing_is_offered_when_nothing_is_being_dragged() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    a.platform.script_outgoing_drag(None);
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, mouser_testkit::Injected::MotionAbs { .. })
    })
    .await;
    settle().await;
    assert!(b.platform.incoming_drags().is_empty());
}

#[tokio::test]
async fn a_taken_name_is_replaced_when_the_user_says_so() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), b"new").unwrap();
    let drop = tempfile::tempdir().unwrap();
    std::fs::write(drop.path().join("a.txt"), b"old").unwrap();
    b.platform.script_drop_target(drop.path().to_path_buf());
    let mut notes = b.engine.control().notifications();
    a.platform
        .script_outgoing_drag(Some(vec![src.path().join("a.txt")]));
    push_right(a);
    let asked = loop {
        let note = recv_topic(&mut notes, Topic::Transfer).await;
        if note["state"] == "awaiting_overwrite_decision" {
            break note;
        }
    };
    assert_eq!(asked["item"], "a.txt");
    // Nothing is replaced before the answer.
    assert_eq!(std::fs::read(drop.path().join("a.txt")).unwrap(), b"old");
    b.engine
        .control()
        .resolve_transfer(asked["transfer_id"].as_str().unwrap(), true);
    loop {
        let note = recv_topic(&mut notes, Topic::Transfer).await;
        if note["state"] == "completed" {
            break;
        }
    }
    assert_eq!(std::fs::read(drop.path().join("a.txt")).unwrap(), b"new");
}

#[tokio::test]
async fn a_platform_without_file_drag_reports_the_limitation_instead() {
    let lab = Lab::new();
    let node = lab
        .start_with("Linux box", VirtualPlatform::limited(), |_| {})
        .await;
    assert_eq!(node.engine.ctx.problems.open(), []);
    let listed = node.engine.ctx.limitations();
    assert!(
        listed.iter().any(|l| l.text.contains("dragged")),
        "the file drag limitation is listed"
    );
    // Nobody can answer a question on a platform that asked none.
    node.engine.control().resolve_transfer("0000", true);
}

#[tokio::test]
async fn transfer_traffic_from_a_computer_that_is_not_on_the_desk_is_refused() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let reject = ControlMessage::TransferReject {
        transfer_id: TransferId([1; 16]),
        reason: "no_space".into(),
    };
    let error = node
        .engine
        .ctx
        .transfer_message(mouser_proto::ids::ComputerId([9; 16]), reject)
        .unwrap_err();
    assert!(error.contains("not on the desk"), "{error}");
}

#[tokio::test]
async fn a_peer_that_breaks_the_protocol_stops_the_transfer_task() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let ctx = node.engine.ctx.clone();
    let drag = std::sync::Arc::new(std::sync::Mutex::new(node.platform.file_drag().unwrap()));
    let transfers = super::engine::Transfers::new(Ids::default(), |_: &std::path::Path| 0);
    let (commands, received) = tokio::sync::mpsc::unbounded_channel();
    let task = tokio::spawn(super::transfer_task(ctx, drag, transfers, received));
    commands
        .send(super::engine::Command::ItemStart {
            id: TransferId([1; 16]),
            index: 0,
        })
        .unwrap();
    assert!(task.await.unwrap_err().is_panic());
}

#[tokio::test]
async fn a_stream_cut_before_its_header_is_dropped() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let conn = a.engine.ctx.core.lock().unwrap().peers[&b.engine.computer_id()]
        .conn
        .clone();
    let mut stream = conn.open_uni().await.unwrap();
    stream.write_all(&[1, 2, 3]).await.unwrap();
    stream.finish().unwrap();
    settle().await;
    // Beta carries on: it still sees Alpha.
    assert_eq!(connected(b).await, 1);
}
