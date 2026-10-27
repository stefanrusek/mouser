//! Targeted paste (Story 6 scenario 3): the hotkey fetches the latest remote
//! content and writes it on this computer only.

use std::collections::BTreeSet;

use mouser_platform::{ClipRep, LocalInput, Platform};
use mouser_proto::control::Method;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::Os;
use mouser_proto::wire::{
    ClipboardKind, ClipboardPayload, ClipboardRepresentation, ControlMessage,
};
use serde_json::json;

use super::files::Inbox;
use super::targeted::{Chord, paste_chord};
use super::{Clip, Command, Out};
use crate::crossing::modifiers::{LEFT_ALT, LEFT_CTRL, LEFT_META, LEFT_SHIFT, RIGHT_CTRL};
use crate::testing::*;

const V: u32 = 0x0007_0019;
const P: u32 = 0x0007_0013;

fn keys(usages: &[u32]) -> BTreeSet<u32> {
    usages.iter().copied().collect()
}

#[test]
fn a_chord_is_modifiers_plus_a_key() {
    let chord = Chord::parse("Ctrl+Alt+Shift+V").unwrap();
    let all = keys(&[LEFT_CTRL, LEFT_ALT, LEFT_SHIFT]);
    assert!(chord.pressed(&all, V));
    assert!(!chord.pressed(&all, P), "another key");
    assert!(
        !chord.pressed(&keys(&[LEFT_CTRL, LEFT_ALT]), V),
        "Shift missing"
    );
    assert!(
        !chord.pressed(&keys(&[LEFT_CTRL, LEFT_ALT, LEFT_SHIFT, LEFT_META]), V),
        "an extra modifier is another chord"
    );
    // Either Ctrl key will do.
    assert!(chord.pressed(&keys(&[RIGHT_CTRL, LEFT_ALT, LEFT_SHIFT]), V));
}

#[test]
fn chords_name_ctrl_alt_shift_and_meta() {
    assert!(
        Chord::parse("Ctrl+V")
            .unwrap()
            .pressed(&keys(&[LEFT_CTRL]), V)
    );
    assert!(
        Chord::parse("Meta+V")
            .unwrap()
            .pressed(&keys(&[LEFT_META]), V)
    );
    assert!(
        Chord::parse("Cmd+V")
            .unwrap()
            .pressed(&keys(&[LEFT_META]), V)
    );
    assert!(Chord::parse("V").unwrap().pressed(&keys(&[]), V));
    assert!(Chord::parse("Ctrl+Alt+P").is_ok());
}

#[test]
fn bad_chords_say_why() {
    assert!(Chord::parse("Ctrl+Nope").unwrap_err().contains("known key"));
    assert!(Chord::parse("Hyper+V").unwrap_err().contains("modifier"));
    assert!(Chord::parse("").unwrap_err().contains("known key"));
}

#[test]
fn paste_is_command_v_on_a_mac_and_control_v_elsewhere() {
    assert!(paste_chord(Os::Macos).pressed(&keys(&[LEFT_META]), V));
    assert!(!paste_chord(Os::Macos).pressed(&keys(&[LEFT_CTRL]), V));
    assert!(paste_chord(Os::Windows).pressed(&keys(&[LEFT_CTRL]), V));
    assert!(paste_chord(Os::Linux).pressed(&keys(&[LEFT_CTRL]), V));
}

fn origin() -> ComputerId {
    ComputerId([1; 16])
}

#[test]
fn targeted_paste_fetches_every_representation_of_the_latest_remote_offer() {
    let platform = mouser_testkit::VirtualPlatform::new();
    let mut clip = platform.clipboard().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut state = Clip::new(&mut *clip, dir.path().to_path_buf(), Inbox::default());
    // Nothing offered yet.
    assert_eq!(
        state.handle(&mut *clip, Command::Targeted).unwrap(),
        Vec::<Out>::new()
    );
    let offer = Command::Message(
        origin(),
        ControlMessage::ClipboardOffer {
            sequence: 3,
            representations: vec![
                ClipboardRepresentation {
                    kind: ClipboardKind::TextPlain,
                    payload: ClipboardPayload::Inline(b"hi".to_vec()),
                },
                ClipboardRepresentation {
                    kind: ClipboardKind::ImagePng,
                    payload: ClipboardPayload::Reference { size: 4 },
                },
            ],
            file_manifest: Vec::new(),
        },
    );
    state.handle(&mut *clip, offer).unwrap();
    // The user copied something else here meanwhile.
    platform.set_local_clipboard(vec![ClipRep::Text("local".into())]);
    state.poll(&mut *clip);
    let fetch = |kind| {
        Out::To(
            origin(),
            ControlMessage::ClipboardFetch { sequence: 3, kind },
        )
    };
    assert_eq!(
        state.handle(&mut *clip, Command::Targeted).unwrap(),
        [
            fetch(ClipboardKind::TextPlain),
            fetch(ClipboardKind::ImagePng)
        ]
    );
    let data = |kind, bytes: &[u8]| {
        Command::Message(
            origin(),
            ControlMessage::ClipboardData {
                sequence: 3,
                kind,
                bytes: bytes.to_vec(),
            },
        )
    };
    state
        .handle(&mut *clip, data(ClipboardKind::TextPlain, b"hi"))
        .unwrap();
    assert_eq!(
        platform.clipboard_contents(),
        [ClipRep::Text("local".into())],
        "written only when all of it is here"
    );
    state
        .handle(&mut *clip, data(ClipboardKind::ImagePng, &[1, 2]))
        .unwrap();
    assert_eq!(
        platform.clipboard_contents(),
        [ClipRep::Text("hi".into()), ClipRep::Png(vec![1, 2])]
    );
    // Tagged as Mouser's own write: no other computer is told.
    assert_eq!(state.poll(&mut *clip), Vec::<Out>::new());
}

#[tokio::test]
async fn the_hotkey_pastes_what_was_copied_elsewhere_and_changes_no_other_clipboard() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let (a, b, c) = (&nodes[0], &nodes[1], &nodes[2]);
    a.platform
        .set_local_clipboard(vec![ClipRep::Text("from Alpha".into())]);
    for node in [b, c] {
        eventually("the copy to reach everyone", || async {
            node.platform.clipboard_contents() == [ClipRep::Text("from Alpha".into())]
        })
        .await;
    }
    // Beta copies something of its own; Alpha and Gamma take it.
    b.platform
        .set_local_clipboard(vec![ClipRep::Text("from Beta".into())]);
    for node in [a, c] {
        eventually("Beta's copy to reach the others", || async {
            node.platform.clipboard_contents() == [ClipRep::Text("from Beta".into())]
        })
        .await;
    }
    let counts = |nodes: &[&TestNode]| -> Vec<u64> {
        nodes
            .iter()
            .map(|n| n.platform.clipboard().unwrap().change_count())
            .collect()
    };
    let before = counts(&[a, c]);
    // Ctrl+Alt+Shift+V on Beta.
    for (usage, down) in [
        (LEFT_CTRL, true),
        (LEFT_ALT, true),
        (LEFT_SHIFT, true),
        (V, true),
        (V, false),
    ] {
        b.platform.send_input(LocalInput::Key { usage, down });
    }
    eventually("Alpha's text on Beta", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("from Alpha".into())]
    })
    .await;
    settle().await;
    assert_eq!(counts(&[a, c]), before, "no other clipboard was written");
    for node in [a, c] {
        assert_eq!(
            node.platform.clipboard_contents(),
            [ClipRep::Text("from Beta".into())]
        );
    }
}

#[tokio::test]
async fn the_hotkey_can_be_changed_and_a_bad_one_is_refused() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let error = call_err(
        b,
        Method::ConfigSet,
        json!({"key": "hotkeys.targeted_paste", "value": "Ctrl+Nope"}),
    )
    .await;
    assert!(
        error.message.starts_with("hotkeys.targeted_paste: "),
        "{}",
        error.message
    );
    let _: mouser_proto::control::Empty = call(
        b,
        Method::ConfigSet,
        json!({"key": "hotkeys.targeted_paste", "value": "Ctrl+Alt+P"}),
    )
    .await;
    a.platform
        .set_local_clipboard(vec![ClipRep::Text("one".into())]);
    eventually("Beta to hold it", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("one".into())]
    })
    .await;
    b.platform
        .set_local_clipboard(vec![ClipRep::Text("two".into())]);
    eventually("Alpha to hold it", || async {
        a.platform.clipboard_contents() == [ClipRep::Text("two".into())]
    })
    .await;
    // The old chord does nothing now; the new one pastes.
    for (usage, down) in [
        (LEFT_CTRL, true),
        (LEFT_ALT, true),
        (LEFT_SHIFT, true),
        (V, true),
        (V, false),
        (LEFT_SHIFT, false),
    ] {
        b.platform.send_input(LocalInput::Key { usage, down });
    }
    settle().await;
    assert_eq!(
        b.platform.clipboard_contents(),
        [ClipRep::Text("two".into())]
    );
    b.platform.send_input(LocalInput::Key {
        usage: P,
        down: true,
    });
    eventually("the new chord to paste", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("one".into())]
    })
    .await;
}

#[tokio::test]
async fn the_hotkey_works_for_a_controlled_computer_too() {
    // Beta is steered from Alpha's keyboard; the chord arrives as forwarded keys.
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    a.platform
        .set_local_clipboard(vec![ClipRep::Text("from Alpha".into())]);
    eventually("Beta to hold it", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("from Alpha".into())]
    })
    .await;
    b.platform
        .set_local_clipboard(vec![ClipRep::Text("from Beta".into())]);
    eventually("Alpha to hold it", || async {
        a.platform.clipboard_contents() == [ClipRep::Text("from Beta".into())]
    })
    .await;
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, mouser_testkit::Injected::MotionAbs { .. })
    })
    .await;
    // Alpha's keyboard now drives Beta: the chord typed there pastes on Beta.
    for (usage, down) in [
        (LEFT_CTRL, true),
        (LEFT_ALT, true),
        (LEFT_SHIFT, true),
        (V, true),
    ] {
        a.platform.send_input(LocalInput::Key { usage, down });
    }
    eventually("the content copied on Alpha on Beta", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("from Alpha".into())]
    })
    .await;
    assert_eq!(
        a.platform.clipboard_contents(),
        [ClipRep::Text("from Beta".into())]
    );
}

#[tokio::test]
async fn a_changed_hotkey_applies_to_forwarded_keys_at_once_and_the_old_one_stops() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    a.platform
        .set_local_clipboard(vec![ClipRep::Text("from Alpha".into())]);
    eventually("Beta to hold it", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("from Alpha".into())]
    })
    .await;
    b.platform
        .set_local_clipboard(vec![ClipRep::Text("from Beta".into())]);
    eventually("Alpha to hold it", || async {
        a.platform.clipboard_contents() == [ClipRep::Text("from Beta".into())]
    })
    .await;
    // Beta is driven from Alpha's keyboard; Beta's hotkey is changed there.
    let _: mouser_proto::control::Empty = call(
        b,
        Method::ConfigSet,
        json!({"key": "hotkeys.targeted_paste", "value": "Ctrl+Alt+P"}),
    )
    .await;
    push_right(a);
    injected_where(&b.platform, "the pointer on Beta", |i| {
        matches!(i, mouser_testkit::Injected::MotionAbs { .. })
    })
    .await;
    let press = |usages: &[u32], key: u32| {
        for usage in usages {
            a.platform.send_input(LocalInput::Key {
                usage: *usage,
                down: true,
            });
        }
        for down in [true, false] {
            a.platform.send_input(LocalInput::Key { usage: key, down });
        }
        for usage in usages.iter().rev() {
            a.platform.send_input(LocalInput::Key {
                usage: *usage,
                down: false,
            });
        }
    };
    // The previous chord no longer pastes on Beta.
    press(&[LEFT_CTRL, LEFT_ALT, LEFT_SHIFT], V);
    settle().await;
    assert_eq!(
        b.platform.clipboard_contents(),
        [ClipRep::Text("from Beta".into())]
    );
    // The new one does.
    press(&[LEFT_CTRL, LEFT_ALT], P);
    eventually("the new chord to paste", || async {
        b.platform.clipboard_contents() == [ClipRep::Text("from Alpha".into())]
    })
    .await;
}
