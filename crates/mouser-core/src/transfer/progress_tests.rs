//! Transfer progress (Story 7 scenario 2): notifications carry the current
//! item, bytes done and total, the rate and the time remaining.

use std::time::{Duration, Instant};

use mouser_proto::wire::ControlMessage;
use serde_json::json;

use super::engine::{Command, Out, State};
use super::engine_tests::*;
use super::progress::{Progress, REPORT_EVERY, notification};

fn after(start: Instant, secs: u64) -> Instant {
    start + Duration::from_secs(secs)
}

#[test]
fn rate_and_time_remaining_follow_from_the_bytes_so_far() {
    let start = Instant::now();
    let mut progress = Progress::new(1000);
    progress.begin_item("big.bin");
    // Nothing yet: no rate to estimate from.
    let snapshot = progress.snapshot(start);
    assert_eq!((snapshot.bytes_done, snapshot.bytes_total), (0, 1000));
    assert_eq!((snapshot.rate, snapshot.eta_seconds), (0, None));
    progress.add(100, start);
    progress.add(150, after(start, 1));
    let snapshot = progress.snapshot(after(start, 2));
    assert_eq!(snapshot.current_item, "big.bin");
    assert_eq!(snapshot.bytes_done, 250);
    assert_eq!(snapshot.rate, 125);
    // 750 left at 125 per second.
    assert_eq!(snapshot.eta_seconds, Some(6));
}

#[test]
fn a_file_left_out_is_not_waited_for() {
    let mut progress = Progress::new(1000);
    progress.skip(400);
    assert_eq!(progress.snapshot(Instant::now()).bytes_total, 600);
}

#[test]
fn progress_is_reported_at_most_every_hundred_milliseconds() {
    let start = Instant::now();
    let mut progress = Progress::new(1);
    assert!(progress.due(start));
    assert!(!progress.due(start + REPORT_EVERY / 2));
    assert!(progress.due(start + REPORT_EVERY));
}

#[test]
fn the_notification_names_the_transfer_state_and_numbers() {
    let mut progress = Progress::new(10);
    progress.begin_item("a.txt");
    let note = notification(&id(), State::Copying, &progress.snapshot(Instant::now()));
    assert_eq!(
        note,
        json!({
            "transfer_id": "09090909090909090909090909090909",
            "state": "copying",
            "current_item": "a.txt",
            "bytes_done": 0,
            "bytes_total": 10,
            "rate": 0,
            "eta_seconds": null,
        })
    );
}

#[test]
fn every_state_has_its_own_name() {
    let names: Vec<_> = [
        State::Pending,
        State::Checking,
        State::Copying,
        State::AwaitingOverwriteDecision,
        State::Completed,
        State::Rejected,
    ]
    .iter()
    .map(|s| s.name())
    .collect();
    assert_eq!(
        names,
        [
            "pending",
            "checking",
            "copying",
            "awaiting_overwrite_decision",
            "completed",
            "rejected"
        ]
    );
}

#[test]
fn arriving_bytes_are_reported_with_the_item_the_speed_and_the_time_left() {
    let (mut t, _) = engine(1 << 20);
    run(&mut t, offer(vec![item("big.bin", 1000, false)]));
    let drop = tempfile::tempdir().unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    run(&mut t, Command::ItemStart { id: id(), index: 0 });
    let start = Instant::now();
    // The first chunk starts the clock and is reported; the next, too soon, is not.
    let first = t
        .handle(
            Command::Chunk {
                id: id(),
                bytes: 100,
            },
            start,
        )
        .unwrap();
    assert!(matches!(&first[..], [Out::Notify(_)]));
    let soon = start + Duration::from_millis(10);
    assert!(
        t.handle(
            Command::Chunk {
                id: id(),
                bytes: 100
            },
            soon
        )
        .unwrap()
        .is_empty()
    );
    let later = after(start, 2);
    let outs = t
        .handle(
            Command::Chunk {
                id: id(),
                bytes: 300,
            },
            later,
        )
        .unwrap();
    let [Out::Notify(note)] = &outs[..] else {
        panic!("one notification: {outs:?}");
    };
    assert_eq!(note["state"], "copying");
    assert_eq!(note["current_item"], "big.bin");
    assert_eq!(note["bytes_done"], 500);
    assert_eq!(note["bytes_total"], 1000);
    assert_eq!(note["rate"], 250);
    assert_eq!(note["eta_seconds"], 2);
    // The last file ending completes the transfer, and says so.
    let outs = run(&mut t, Command::ItemDone { id: id() });
    let [
        Out::To(_, ControlMessage::TransferDone { .. }),
        Out::Notify(note),
    ] = &outs[..]
    else {
        panic!("done and a final notification: {outs:?}");
    };
    assert_eq!(note["state"], "completed");
}

#[test]
fn a_finished_file_is_reported_while_others_remain() {
    let (mut t, _) = engine(1 << 20);
    run(
        &mut t,
        offer(vec![item("a", 1, false), item("b", 1, false)]),
    );
    let drop = tempfile::tempdir().unwrap();
    run(
        &mut t,
        Command::Dropped {
            id: id(),
            dir: drop.path().to_path_buf(),
        },
    );
    run(&mut t, Command::ItemStart { id: id(), index: 0 });
    let outs = run(&mut t, Command::ItemDone { id: id() });
    assert!(matches!(&outs[..], [Out::Notify(note)] if note["state"] == "copying"));
}
