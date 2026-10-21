//! Display removal and undo (Story 4 scenario 3, edge case).

use mouser_proto::control::{Empty, Method};
use serde_json::{Value, json};

use crate::testing::*;

fn without(layout: &Value, key: &str) -> Value {
    let mut next = layout.clone();
    next["placements"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["display_id"]["key"] != key);
    next
}

fn inclusion(layout: &mut Value, key: &str, computer: &Value, included: bool) {
    layout["displays"] = json!([{
        "display_id": {"computer": computer, "key": key},
        "included": included
    }]);
}

#[tokio::test]
async fn a_removed_display_leaves_every_computers_map_and_removal_can_be_undone() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let full = layout_of(b).await;
    let beta = json!(b.engine.computer_id().to_string());
    let epoch = epoch_of(b);

    let mut removal = without(&full, "b2");
    inclusion(&mut removal, "b2", &beta, false);
    let _: Empty = call(b, Method::LayoutSet, removal).await;
    for node in [a, b] {
        eventually("b2 off the map", || async {
            mapped(node) == ["b1", "virtual-0"]
        })
        .await;
    }
    assert_eq!(epoch_of(b), epoch + 1);
    let seen = layout_of(a).await;
    let b2 = seen["displays"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["display_id"]["key"] == "b2")
        .unwrap();
    assert_eq!(b2["included"], false);
    assert!(
        seen["placements"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["display_id"]["key"] != "b2")
    );
    // The pointer cannot cross onto it: nothing adjoins b1's right edge.
    assert_eq!(a.engine.layout(), b.engine.layout());

    let mut undo = full.clone();
    inclusion(&mut undo, "b2", &beta, true);
    let _: Empty = call(b, Method::LayoutSet, undo).await;
    for node in [a, b] {
        eventually("b2 back on the map", || async { mapped(node).len() == 3 }).await;
    }
    assert_eq!(layout_of(a).await["placements"], full["placements"]);
}

#[tokio::test]
async fn only_the_owning_computer_removes_a_display() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let full = layout_of(&nodes[0]).await;
    let beta = json!(nodes[1].engine.computer_id().to_string());
    let mut removal = without(&full, "b2");
    inclusion(&mut removal, "b2", &beta, false);
    let error = call_err(&nodes[0], Method::LayoutSet, removal).await;
    assert_eq!(
        error.message,
        "b2 belongs to another computer; remove it on that computer"
    );
    assert_eq!(layout_of(&nodes[0]).await, full);
}

#[tokio::test]
async fn removal_that_would_cut_off_a_display_is_refused() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let full = layout_of(&nodes[1]).await;
    let beta = json!(nodes[1].engine.computer_id().to_string());
    let mut removal = without(&full, "b1");
    inclusion(&mut removal, "b1", &beta, false);
    // Alpha | b1 | b2: with b1 gone, b2 touches nothing.
    let error = call_err(&nodes[1], Method::LayoutSet, removal).await;
    assert!(
        error.message.contains("cannot be reached"),
        "{}",
        error.message
    );
    assert_eq!(layout_of(&nodes[1]).await, full);
}

#[tokio::test]
async fn a_computer_with_every_display_removed_stays_a_member() {
    let lab = Lab::new();
    let nodes = docked_desk(&lab).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let mut layout = layout_of(b).await;
    let beta = json!(b.engine.computer_id().to_string());
    layout["placements"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["display_id"]["computer"] != beta);
    layout["displays"] = json!([
        {"display_id": {"computer": beta, "key": "b1"}, "included": false},
        {"display_id": {"computer": beta, "key": "b2"}, "included": false},
    ]);
    let _: Empty = call(b, Method::LayoutSet, layout).await;
    for node in [a, b] {
        eventually("only Alpha on the map", || async {
            mapped(node) == ["virtual-0"]
        })
        .await;
    }
    settle().await;
    // Still on the desk and connected, so it still takes part in everything
    // that goes to the desk's members.
    assert_eq!(connected(a).await, 1);
    assert_eq!(connected(b).await, 1);
    assert_eq!(status(a).await.computers.len(), 2);
    assert_eq!(a.engine.layout().placements.len(), 1);
}

#[tokio::test]
async fn removed_displays_are_remembered_across_restarts() {
    let lab = Lab::new();
    let mut nodes = docked_desk(&lab).await;
    let b = nodes.pop().unwrap();
    let full = layout_of(&b).await;
    let beta = json!(b.engine.computer_id().to_string());
    let mut removal = without(&full, "b2");
    inclusion(&mut removal, "b2", &beta, false);
    let _: Empty = call(&b, Method::LayoutSet, removal).await;
    let (platform, dir) = (b.platform.clone(), b.dir);
    b.engine.shutdown().await;
    let b = lab.boot("Beta", platform, dir).await;
    let seen = layout_of(&b).await;
    let b2 = seen["displays"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["display_id"]["key"] == "b2")
        .unwrap();
    assert_eq!(b2["included"], false);
}

mod apply {
    use std::collections::{BTreeMap, BTreeSet};

    use mouser_proto::ids::ComputerId;
    use mouser_proto::model::{Display, DisplayId};

    use crate::layout::remove::{Inclusion, apply};

    fn display(computer: u8, key: &str) -> Display {
        Display {
            display_id: DisplayId {
                computer: ComputerId([computer; 16]),
                key: key.into(),
            },
            name: key.into(),
            pixel_size: (100, 100),
            logical_size: (100.0, 100.0),
            scale: 1.0,
            physical_size_mm: None,
            origin_local: (0, 0),
            included: true,
        }
    }

    fn change(computer: u8, key: &str, included: bool) -> Inclusion {
        Inclusion {
            display_id: display(computer, key).display_id,
            included,
        }
    }

    #[test]
    fn an_owner_removes_and_restores_its_own_displays() {
        let me = ComputerId([1; 16]);
        let mut known = BTreeMap::from([(me, vec![display(1, "a")])]);
        let mut removed = BTreeSet::new();
        apply(me, &mut known, &mut removed, &[change(1, "a", false)]).unwrap();
        assert!(!known[&me][0].included);
        assert_eq!(removed, BTreeSet::from(["a".to_string()]));
        // Saying it again changes nothing.
        apply(me, &mut known, &mut removed, &[change(1, "a", false)]).unwrap();
        assert_eq!(removed.len(), 1);
        apply(me, &mut known, &mut removed, &[change(1, "a", true)]).unwrap();
        assert!(known[&me][0].included);
        assert!(removed.is_empty());
    }

    #[test]
    fn unknown_displays_and_other_computers_displays_are_refused() {
        let me = ComputerId([1; 16]);
        let mut known = BTreeMap::from([
            (me, vec![display(1, "a")]),
            (ComputerId([2; 16]), vec![display(2, "b")]),
        ]);
        let mut removed = BTreeSet::new();
        for (c, k) in [(1, "nope"), (3, "a")] {
            let error = apply(me, &mut known, &mut removed, &[change(c, k, false)]).unwrap_err();
            assert_eq!(error, format!("{k} is not a display on this desk"));
        }
        let error = apply(me, &mut known, &mut removed, &[change(2, "b", false)]).unwrap_err();
        assert_eq!(
            error,
            "b belongs to another computer; remove it on that computer"
        );
        // Including what is already included is not a removal.
        apply(me, &mut known, &mut removed, &[change(2, "b", true)]).unwrap();
        assert!(removed.is_empty());
    }
}
