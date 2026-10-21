//! Applying offers (Story 6 scenario 1 and the unrepresentable-format edge
//! case): small representations arrive inline and are all written, large ones
//! are fetched on paste, and what the receiver cannot hold is dropped.

use mouser_platform::{ClipRep, Clipboard, LocalInput, Platform, Result};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{
    ClipboardKind, ClipboardPayload, ClipboardRepresentation, ControlMessage,
};
use mouser_testkit::VirtualPlatform;

use super::apply::write_best;
use super::files::Inbox;
use super::{Clip, Command, INLINE_LIMIT, Out};
use crate::crossing::modifiers::LEFT_CTRL;
use crate::testing::*;

const V: u32 = 0x0007_0019;

fn origin() -> ComputerId {
    ComputerId([1; 16])
}

fn inline(kind: ClipboardKind, bytes: &[u8]) -> ClipboardRepresentation {
    ClipboardRepresentation {
        kind,
        payload: ClipboardPayload::Inline(bytes.to_vec()),
    }
}

fn reference(kind: ClipboardKind, size: u64) -> ClipboardRepresentation {
    ClipboardRepresentation {
        kind,
        payload: ClipboardPayload::Reference { size },
    }
}

fn offer(sequence: u64, representations: Vec<ClipboardRepresentation>) -> Command {
    Command::Message(
        origin(),
        ControlMessage::ClipboardOffer {
            sequence,
            representations,
            file_manifest: Vec::new(),
        },
    )
}

fn data(sequence: u64, kind: ClipboardKind, bytes: &[u8]) -> Command {
    Command::Message(
        origin(),
        ControlMessage::ClipboardData {
            sequence,
            kind,
            bytes: bytes.to_vec(),
        },
    )
}

struct Rig {
    platform: VirtualPlatform,
    clip: Box<dyn Clipboard>,
    state: Clip,
    _dir: tempfile::TempDir,
}

fn rig() -> Rig {
    let platform = VirtualPlatform::new();
    let mut clip = platform.clipboard().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let state = Clip::new(&mut *clip, dir.path().to_path_buf(), Inbox::default());
    Rig {
        platform,
        clip,
        state,
        _dir: dir,
    }
}

impl Rig {
    fn handle(&mut self, command: Command) -> Vec<Out> {
        self.state.handle(&mut *self.clip, command).unwrap()
    }
}

#[test]
fn small_representations_arrive_inline_and_are_all_written_at_once() {
    let mut rig = rig();
    let outs = rig.handle(offer(
        1,
        vec![
            inline(ClipboardKind::TextPlain, b"hello"),
            inline(ClipboardKind::TextHtml, b"<b>hello</b>"),
            inline(ClipboardKind::TextRtf, b"{\\rtf1 hello}"),
            inline(ClipboardKind::ImagePng, &[1, 2, 3]),
        ],
    ));
    assert_eq!(outs, none());
    assert_eq!(
        rig.platform.clipboard_contents(),
        [
            ClipRep::Text("hello".into()),
            ClipRep::Html("<b>hello</b>".into()),
            ClipRep::Rtf(b"{\\rtf1 hello}".to_vec()),
            ClipRep::Png(vec![1, 2, 3]),
        ]
    );
    // Written by Mouser, so not offered on to anyone.
    assert_eq!(rig.state.poll(&mut *rig.clip), none());
}

#[test]
fn a_large_representation_is_fetched_when_the_user_pastes() {
    let mut rig = rig();
    let size = INLINE_LIMIT as u64 + 1;
    rig.handle(offer(
        7,
        vec![
            inline(ClipboardKind::TextPlain, b"caption"),
            reference(ClipboardKind::ImagePng, size),
        ],
    ));
    // The small part is there already; the image is not.
    assert_eq!(
        rig.platform.clipboard_contents(),
        [ClipRep::Text("caption".into())]
    );
    let fetch = Out::To(
        origin(),
        ControlMessage::ClipboardFetch {
            sequence: 7,
            kind: ClipboardKind::ImagePng,
        },
    );
    assert_eq!(rig.handle(Command::Paste), [fetch]);
    // Pasting again while it is on its way asks for nothing more.
    assert_eq!(rig.handle(Command::Paste), none());
    assert_eq!(
        rig.handle(data(7, ClipboardKind::ImagePng, &[9, 9])),
        none()
    );
    assert_eq!(
        rig.platform.clipboard_contents(),
        [ClipRep::Text("caption".into()), ClipRep::Png(vec![9, 9])]
    );
    // Everything is here now: pasting fetches nothing, and nothing is offered on.
    assert_eq!(rig.handle(Command::Paste), none());
    assert_eq!(rig.state.poll(&mut *rig.clip), none());
}

#[test]
fn pasting_with_nothing_to_fetch_does_nothing() {
    let mut rig = rig();
    assert_eq!(rig.handle(Command::Paste), none());
    rig.handle(offer(1, vec![inline(ClipboardKind::TextPlain, b"x")]));
    assert_eq!(rig.handle(Command::Paste), none());
}

#[test]
fn data_nobody_is_waiting_for_is_ignored() {
    let mut rig = rig();
    rig.handle(offer(2, vec![reference(ClipboardKind::ImagePng, 5)]));
    // Not fetching yet.
    rig.handle(data(2, ClipboardKind::ImagePng, &[1]));
    assert_eq!(rig.platform.clipboard_contents(), nothing());
    rig.handle(Command::Paste);
    // Wrong sequence, wrong kind, wrong computer.
    rig.handle(data(1, ClipboardKind::ImagePng, &[1]));
    rig.handle(data(2, ClipboardKind::TextHtml, b"x"));
    rig.handle(Command::Message(
        ComputerId([2; 16]),
        ControlMessage::ClipboardData {
            sequence: 2,
            kind: ClipboardKind::ImagePng,
            bytes: vec![1],
        },
    ));
    assert_eq!(rig.platform.clipboard_contents(), nothing());
    rig.handle(data(2, ClipboardKind::ImagePng, &[5]));
    assert_eq!(rig.platform.clipboard_contents(), [ClipRep::Png(vec![5])]);
}

#[test]
fn copying_here_cancels_what_was_still_to_be_fetched() {
    let mut rig = rig();
    rig.handle(offer(1, vec![reference(ClipboardKind::ImagePng, 5)]));
    rig.platform
        .set_local_clipboard(vec![ClipRep::Text("mine".into())]);
    assert_eq!(
        rig.state.poll(&mut *rig.clip).len(),
        1,
        "the copy is offered"
    );
    assert_eq!(rig.handle(Command::Paste), none());
}

#[test]
fn a_computer_that_went_away_leaves_nothing_to_fetch() {
    let mut rig = rig();
    rig.handle(offer(1, vec![reference(ClipboardKind::ImagePng, 5)]));
    rig.handle(Command::Paste);
    rig.handle(Command::PeerGone(ComputerId([2; 16])));
    assert_eq!(
        rig.handle(Command::Paste),
        [],
        "still fetching from the origin"
    );
    rig.handle(Command::PeerGone(origin()));
    assert_eq!(rig.handle(Command::Paste), none());
    assert_eq!(rig.handle(Command::Targeted), none());
    rig.handle(data(1, ClipboardKind::ImagePng, &[5]));
    assert_eq!(rig.platform.clipboard_contents(), nothing());
}

#[test]
fn other_messages_are_not_clipboard_traffic() {
    let mut rig = rig();
    let result = rig.state.handle(
        &mut *rig.clip,
        Command::Message(origin(), ControlMessage::Panic),
    );
    assert!(result.unwrap_err().contains("unexpected"));
}

type Holds = fn(&ClipRep) -> bool;

/// A clipboard that holds only some kinds of content.
struct Picky {
    holds: Holds,
    written: Option<Vec<ClipRep>>,
}

impl Clipboard for Picky {
    fn read(&mut self) -> Result<Vec<ClipRep>> {
        Ok(self.written.clone().unwrap_or_default())
    }

    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()> {
        if reps.iter().all(self.holds) {
            self.written = Some(reps);
            Ok(())
        } else {
            Err(mouser_platform::Error::Unsupported)
        }
    }

    fn change_count(&mut self) -> u64 {
        0
    }
}

fn mixed() -> Vec<ClipRep> {
    vec![
        ClipRep::Text("plain".into()),
        ClipRep::Html("<b>rich</b>".into()),
        ClipRep::Rtf(b"{\\rtf1 rich}".to_vec()),
        ClipRep::Png(vec![1]),
        ClipRep::Files(vec!["/tmp/a".into()]),
    ]
}

#[test]
fn what_the_receiver_cannot_hold_is_dropped_and_the_best_remaining_is_written() {
    let cases: [(Holds, Vec<ClipRep>); 5] = [
        (|_| true, mixed()),
        (
            |r| !matches!(r, ClipRep::Rtf(_)),
            vec![
                ClipRep::Text("plain".into()),
                ClipRep::Html("<b>rich</b>".into()),
                ClipRep::Png(vec![1]),
                ClipRep::Files(vec!["/tmp/a".into()]),
            ],
        ),
        (
            |r| matches!(r, ClipRep::Text(_) | ClipRep::Png(_)),
            vec![ClipRep::Text("plain".into()), ClipRep::Png(vec![1])],
        ),
        (
            |r| matches!(r, ClipRep::Files(_) | ClipRep::Text(_)),
            vec![
                ClipRep::Text("plain".into()),
                ClipRep::Files(vec!["/tmp/a".into()]),
            ],
        ),
        (
            |r| matches!(r, ClipRep::Text(_)),
            vec![ClipRep::Text("plain".into())],
        ),
    ];
    for (holds, expected) in cases {
        let mut picky = Picky {
            holds,
            written: None,
        };
        write_best(&mut picky, mixed());
        assert_eq!(picky.written, Some(expected));
    }
}

#[test]
fn nothing_is_written_when_nothing_can_be_held() {
    let mut picky = Picky {
        holds: |_| false,
        written: None,
    };
    write_best(&mut picky, mixed());
    assert_eq!(picky.written, None);
}

#[test]
fn formatted_text_the_receiver_cannot_hold_is_pasted_as_plain_text() {
    let mut picky = Picky {
        holds: |r| matches!(r, ClipRep::Text(_)),
        written: None,
    };
    let dir = tempfile::tempdir().unwrap();
    let mut state = Clip::new(&mut picky, dir.path().to_path_buf(), Inbox::default());
    state
        .handle(
            &mut picky,
            offer(
                1,
                vec![
                    inline(ClipboardKind::TextPlain, b"words"),
                    inline(ClipboardKind::TextHtml, b"<b>words</b>"),
                ],
            ),
        )
        .unwrap();
    assert_eq!(picky.written, Some(vec![ClipRep::Text("words".into())]));
}

#[tokio::test]
async fn pasting_on_the_receiving_computer_fetches_the_large_part() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let big = vec![3u8; INLINE_LIMIT + 10];
    a.platform.set_local_clipboard(vec![
        ClipRep::Text("caption".into()),
        ClipRep::Png(big.clone()),
    ]);
    eventually("Beta to hold the caption", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("caption".into())]
    })
    .await;
    // Ctrl+V on Beta's own keyboard.
    for (usage, down) in [(LEFT_CTRL, true), (V, true), (V, false), (LEFT_CTRL, false)] {
        b.platform.send_input(LocalInput::Key { usage, down });
    }
    eventually("the image to arrive", || async {
        b.platform.clipboard_contents()
            == [ClipRep::Text("caption".into()), ClipRep::Png(big.clone())]
    })
    .await;
    // And it was not offered back to Alpha.
    settle().await;
    assert_eq!(a.platform.clipboard().unwrap().change_count(), 1);
}

fn none() -> Vec<Out> {
    Vec::new()
}

fn nothing() -> Vec<ClipRep> {
    Vec::new()
}

#[test]
fn the_origin_serves_what_it_offered_to_whoever_asks() {
    let mut rig = rig();
    rig.platform.set_local_clipboard(vec![
        ClipRep::Text("hello".into()),
        ClipRep::Png(vec![1, 2]),
    ]);
    rig.state.poll(&mut *rig.clip);
    let fetch = |sequence, kind| {
        Command::Message(origin(), ControlMessage::ClipboardFetch { sequence, kind })
    };
    assert_eq!(
        rig.handle(fetch(1, ClipboardKind::ImagePng)),
        [Out::To(
            origin(),
            ControlMessage::ClipboardData {
                sequence: 1,
                kind: ClipboardKind::ImagePng,
                bytes: vec![1, 2]
            }
        )]
    );
    // An old sequence, a kind that was not copied, and nothing offered yet.
    assert_eq!(rig.handle(fetch(0, ClipboardKind::ImagePng)), none());
    assert_eq!(rig.handle(fetch(1, ClipboardKind::TextHtml)), none());
    let mut fresh = self::rig();
    assert_eq!(fresh.handle(fetch(1, ClipboardKind::TextPlain)), none());
    assert_eq!(fresh.platform.clipboard_contents(), nothing());
}

#[tokio::test]
async fn clipboard_traffic_from_a_computer_that_is_not_on_the_desk_is_refused() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let offer = ControlMessage::ClipboardOffer {
        sequence: 1,
        representations: Vec::new(),
        file_manifest: Vec::new(),
    };
    let error = node
        .engine
        .ctx
        .clipboard_message(ComputerId([9; 16]), offer)
        .unwrap_err();
    assert!(error.contains("not on the desk"), "{error}");
}

#[tokio::test]
async fn what_is_meant_for_a_computer_that_is_gone_goes_nowhere() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let gone = ComputerId([9; 16]);
    node.engine.ctx.emit(vec![
        Out::To(gone, ControlMessage::Panic),
        Out::All(ControlMessage::Panic),
        Out::Stream {
            to: gone,
            transfer: mouser_proto::wire::TransferId([1; 16]),
            files: Vec::new(),
        },
    ]);
}
