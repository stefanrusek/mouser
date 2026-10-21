//! Clipboard watching and offers (Story 6): a `change_count` increase makes an
//! offer with a growing sequence, and Mouser's own writes are never offered.

use mouser_platform::{ClipRep, Platform};
use mouser_proto::wire::{ClipboardKind, ClipboardPayload, ControlMessage};
use mouser_testkit::VirtualPlatform;

use super::watch::{Watcher, offer};
use super::{INLINE_LIMIT, decode, encode, kind_of};
use crate::testing::*;

fn text(s: &str) -> Vec<ClipRep> {
    vec![ClipRep::Text(s.into())]
}

#[test]
fn a_change_count_increase_reads_the_clipboard_with_an_increasing_sequence() {
    let platform = VirtualPlatform::new();
    let mut clip = platform.clipboard().unwrap();
    // What was on the clipboard before Mouser started is not shared.
    platform.set_local_clipboard(text("old"));
    let mut watcher = Watcher::new(&mut *clip);
    assert_eq!(watcher.poll(&mut *clip), None);
    platform.set_local_clipboard(text("one"));
    assert_eq!(watcher.poll(&mut *clip), Some((1, text("one"))));
    assert_eq!(watcher.poll(&mut *clip), None, "nothing changed since");
    platform.set_local_clipboard(text("two"));
    assert_eq!(watcher.poll(&mut *clip), Some((2, text("two"))));
}

#[test]
fn writes_mouser_makes_itself_are_never_offered() {
    let platform = VirtualPlatform::new();
    let mut clip = platform.clipboard().unwrap();
    let mut watcher = Watcher::new(&mut *clip);
    clip.write(text("from elsewhere")).unwrap();
    watcher.wrote(&mut *clip);
    assert_eq!(watcher.poll(&mut *clip), None);
    // A copy by the user right after is still seen.
    platform.set_local_clipboard(text("mine"));
    assert_eq!(watcher.poll(&mut *clip), Some((1, text("mine"))));
}

#[test]
fn a_cleared_clipboard_is_not_offered() {
    let platform = VirtualPlatform::new();
    let mut clip = platform.clipboard().unwrap();
    let mut watcher = Watcher::new(&mut *clip);
    platform.set_local_clipboard(Vec::new());
    assert_eq!(watcher.poll(&mut *clip), None);
    platform.set_local_clipboard(text("again"));
    assert_eq!(watcher.poll(&mut *clip), Some((1, text("again"))));
}

#[test]
fn representations_up_to_one_mebibyte_travel_inline_and_larger_ones_by_reference() {
    let exactly = vec![7u8; INLINE_LIMIT];
    let over = vec![9u8; INLINE_LIMIT + 1];
    let reps = vec![
        ClipRep::Text("hello".into()),
        ClipRep::Html("<b>hello</b>".into()),
        ClipRep::Rtf(b"{\\rtf1 hello}".to_vec()),
        ClipRep::Png(exactly.clone()),
    ];
    let (message, served) = offer(4, &reps);
    let ControlMessage::ClipboardOffer {
        sequence,
        representations,
        file_manifest,
    } = message
    else {
        panic!("an offer");
    };
    assert_eq!(sequence, 4);
    assert!(file_manifest.is_empty());
    assert_eq!(
        representations.iter().map(|r| r.kind).collect::<Vec<_>>(),
        [
            ClipboardKind::TextPlain,
            ClipboardKind::TextHtml,
            ClipboardKind::TextRtf,
            ClipboardKind::ImagePng
        ]
    );
    assert!(
        representations
            .iter()
            .all(|r| matches!(&r.payload, ClipboardPayload::Inline(_)))
    );
    assert_eq!(
        representations[3].payload,
        ClipboardPayload::Inline(exactly)
    );
    assert_eq!(served.sequence, 4);
    assert_eq!(served.reps, reps);

    let (message, _) = offer(5, &[ClipRep::Png(over)]);
    let ControlMessage::ClipboardOffer {
        representations, ..
    } = message
    else {
        panic!("an offer");
    };
    assert_eq!(
        representations[0].payload,
        ClipboardPayload::Reference {
            size: INLINE_LIMIT as u64 + 1
        }
    );
}

#[test]
fn kinds_and_bytes_round_trip() {
    for rep in [
        ClipRep::Text("héllo".into()),
        ClipRep::Html("<i>x</i>".into()),
        ClipRep::Rtf(vec![1, 2, 3]),
        ClipRep::Png(vec![4, 5]),
    ] {
        assert_eq!(decode(kind_of(&rep), encode(&rep).unwrap()), rep);
    }
    assert_eq!(kind_of(&ClipRep::Files(Vec::new())), ClipboardKind::Files);
    assert_eq!(encode(&ClipRep::Files(Vec::new())), None);
}

#[test]
#[should_panic(expected = "files are never carried inline")]
fn files_are_never_decoded_from_bytes() {
    decode(ClipboardKind::Files, Vec::new());
}

#[tokio::test]
async fn a_copy_is_offered_to_the_desk_and_the_receivers_write_is_not_echoed() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    a.platform.set_local_clipboard(text("hello"));
    eventually("Beta to hold the text", || async {
        b.platform.clipboard_contents() == text("hello")
    })
    .await;
    // Beta's write is not offered back: Alpha's clipboard is written once, by the user.
    settle().await;
    let mut alpha = a.platform.clipboard().unwrap();
    assert_eq!(alpha.change_count(), 1);
    assert_eq!(a.platform.clipboard_contents(), text("hello"));
}
