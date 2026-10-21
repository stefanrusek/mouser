use std::collections::BTreeSet;

use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId, Layout, Placement};

use super::autoplace::*;
use super::edges::{Placed, Rect};

fn display(computer: u8, key: &str, origin: (i32, i32)) -> Display {
    Display {
        display_id: DisplayId {
            computer: ComputerId([computer; 16]),
            key: key.into(),
        },
        name: key.into(),
        pixel_size: (1920, 1080),
        logical_size: (1920.0, 1080.0),
        scale: 1.0,
        physical_size_mm: Some((527.0, 296.0)),
        origin_local: origin,
        included: true,
    }
}

fn kept(x: i32, y: i32, w: i32, h: i32) -> Placed {
    Placed {
        display: display(1, "k", (0, 0)),
        rect: Rect { x, y, w, h },
    }
}

fn at(p: &[Placement], key: &str) -> (i32, i32) {
    let p = p.iter().find(|p| p.display_id.key == key).unwrap();
    (p.x, p.y)
}

#[test]
fn the_first_displays_start_at_the_origin() {
    let placed = place(&[], &[display(1, "a", (0, 0))]);
    assert_eq!(at(&placed, "a"), (0, 0));
}

#[test]
fn new_displays_go_right_of_the_rightmost_display_top_aligned_with_it() {
    // Two kept displays: 0..527 and 600..1100, tops at 10 and 40.
    let kept = [kept(0, 10, 527, 296), kept(600, 40, 500, 300)];
    let placed = place(&kept, &[display(2, "b", (-100, 50))]);
    assert_eq!(at(&placed, "b"), (1100, 40));
}

#[test]
fn the_new_computer_adjoins_the_layout_even_when_its_top_display_is_not_rightmost() {
    // A wide display above a narrower one that reaches 3 mm further right
    // (the bounding box's top-right corner touches neither).
    let kept = [kept(0, 0, 801, 339), kept(205, 339, 599, 340)];
    // The new computer's leftmost display is its lower one.
    let new = [display(2, "high", (1920, -1080)), display(2, "low", (0, 0))];
    let placed = place(&kept, &new);
    assert_eq!(at(&placed, "low"), (804, 339));
    assert_eq!(at(&placed, "high"), (804 + 527, 339 - 296));
}

/// A display of `logical` points and `mm` size at `origin`.
fn sized(key: &str, origin: (i32, i32), logical: (f32, f32), mm: (f32, f32)) -> Display {
    Display {
        logical_size: logical,
        physical_size_mm: Some(mm),
        ..display(2, key, origin)
    }
}

#[test]
fn displays_that_touch_locally_still_touch_with_mixed_densities() {
    // A Mac mini's 5K display (2560 pt, 599 mm) with an ultrawide (3440 pt,
    // 802 mm) above it and 880 pt further left: the two densities differ, so
    // scaling the whole arrangement by one of them leaves a gap or overlap.
    let new = [
        sized("5k", (0, 0), (2560.0, 1440.0), (599.3, 340.2)),
        sized("wide", (-880, -1440), (3440.0, 1440.0), (801.6, 338.7)),
    ];
    let placed = place(&[], &new);
    assert_eq!(at(&placed, "wide"), (0, 0));
    assert_eq!(at(&placed, "5k"), (205, 339));
}

#[test]
fn neighbours_on_every_side_are_placed_against_the_shared_edge() {
    // m in the middle; l left, r right, t above, b below, each offset along
    // the edge by 100 pt (27.4 mm at 527 mm per 1920 pt; 296 mm per 1080 pt).
    let new = [
        display(2, "m", (0, 0)),
        display(2, "l", (-1920, 100)),
        display(2, "r", (1920, 100)),
        display(2, "t", (100, -1080)),
        display(2, "b", (100, 1080)),
    ];
    let placed = place(&[], &new);
    let m = at(&placed, "m");
    let rel = |k| {
        let (x, y) = at(&placed, k);
        (x - m.0, y - m.1)
    };
    assert_eq!(rel("l"), (-527, 27));
    assert_eq!(rel("r"), (527, 27));
    assert_eq!(rel("t"), (27, -296));
    assert_eq!(rel("b"), (27, 296));
}

#[test]
fn a_display_reached_only_through_its_right_neighbour_is_placed_against_it() {
    // a, then c to its right and d below c; e is left of d but 120 pt below
    // a, so it is placed from d.
    let new = [
        display(2, "a", (0, 0)),
        display(2, "c", (1920, 0)),
        display(2, "d", (1920, 1080)),
        display(2, "e", (0, 1200)),
    ];
    let placed = place(&[], &new);
    assert_eq!(at(&placed, "d"), (527, 296));
    assert_eq!(at(&placed, "e"), (0, 296 + 33));
}

#[test]
fn a_display_touching_none_keeps_its_offset_from_the_first() {
    let new = [display(2, "a", (0, 0)), display(2, "far", (3840, 2160))];
    let placed = place(&[], &new);
    assert_eq!(at(&placed, "far"), (1054, 592));
}

fn kept_of(computer: u8, x: i32, y: i32) -> Placed {
    Placed {
        display: display(computer, "k", (0, 0)),
        rect: Rect {
            x,
            y,
            w: 527,
            h: 296,
        },
    }
}

#[test]
fn computers_whose_displays_adjoin_no_other_computer_are_isolated() {
    // 1 and 2 adjoin; 3 floats apart.
    let placed = [kept_of(1, 0, 0), kept_of(2, 527, 0), kept_of(3, 2000, 0)];
    assert_eq!(isolated(&placed), BTreeSet::from([ComputerId([3; 16])]));
    // A computer alone on the desk is not isolated from anyone.
    assert!(isolated(&[kept_of(1, 0, 0)]).is_empty());
    // Two that touch nothing are both isolated.
    assert_eq!(isolated(&[kept_of(1, 0, 0), kept_of(2, 600, 0)]).len(), 2);
}

#[test]
fn the_local_arrangement_is_kept() {
    // b sits right of a and lower by 540 px: 1920 px = 527 mm, 540 px = 148.
    let new = [display(2, "b", (1920, 540)), display(2, "a", (0, 0))];
    let placed = place(&[kept(0, 0, 100, 100)], &new);
    assert_eq!(at(&placed, "a"), (100, 0));
    assert_eq!(at(&placed, "b"), (100 + 527, 148));
    // Origins may be negative: only relative positions matter.
    let negative = [display(2, "l", (-1920, 0)), display(2, "m", (0, 0))];
    let placed = place(&[], &negative);
    assert_eq!((at(&placed, "l"), at(&placed, "m")), ((0, 0), (527, 0)));
}

#[test]
fn nothing_to_place_places_nothing() {
    assert!(place(&[], &[]).is_empty());
}

fn layout(at: &[(u8, &str)]) -> Layout {
    Layout {
        placements: at
            .iter()
            .map(|(c, k)| Placement {
                display_id: DisplayId {
                    computer: ComputerId([*c; 16]),
                    key: (*k).into(),
                },
                x: 0,
                y: 0,
            })
            .collect(),
        ..Layout::default()
    }
}

#[test]
fn a_review_is_offered_for_new_and_replaced_displays_only() {
    let old = layout(&[(1, "a"), (2, "b")]);
    // 3 is new, 2 was docked (its set changed), 1 is untouched.
    let new = layout(&[(1, "a"), (2, "c"), (3, "d")]);
    assert_eq!(
        review_candidates(&old, &new),
        [ComputerId([2; 16]), ComputerId([3; 16])]
    );
    // Moving a display around changes nothing that needs review.
    assert!(review_candidates(&old, &old).is_empty());
}

#[test]
fn the_very_first_placement_needs_no_review() {
    assert!(review_candidates(&Layout::default(), &layout(&[(1, "a")])).is_empty());
}

mod on_the_wire {
    use crate::EngineEvent;
    use crate::testing::*;

    #[tokio::test]
    async fn a_new_computer_is_placed_right_of_the_layout_and_the_owner_is_offered_a_review() {
        let lab = Lab::new();
        let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let mut events = nodes[0].engine.events();
        let gamma = lab.start("Gamma").await;
        admit(&nodes[0], &gamma).await;
        nodes.push(gamma);
        ready(&nodes).await;
        let EngineEvent::ReviewOffered { computer, name } = events.recv().await.unwrap() else {
            panic!("a review is offered");
        };
        assert_eq!(
            (computer, name.as_str()),
            (nodes[2].engine.computer_id(), "Gamma")
        );
        // Alpha at 0, Beta (527 mm wide) after it, Gamma after both, tops aligned.
        let layout = nodes[2].engine.layout();
        let mut at: Vec<_> = layout
            .placements
            .iter()
            .map(|p| (p.x, p.y, p.display_id.computer))
            .collect();
        at.sort();
        assert_eq!(
            at.iter().map(|(x, y, _)| (*x, *y)).collect::<Vec<_>>(),
            [(0, 0), (527, 0), (1054, 0)]
        );
        assert_eq!(at[2].2, nodes[2].engine.computer_id());
        for node in &nodes {
            assert_eq!(node.engine.layout(), layout, "every member agrees");
        }
        let flagged = nodes[0].engine.needs_review();
        assert!(
            flagged
                .iter()
                .any(|d| d.computer == nodes[2].engine.computer_id())
        );
        assert_eq!(
            flagged.len(),
            2,
            "Beta joined earlier and is still unreviewed"
        );
    }

    #[tokio::test]
    async fn a_computer_left_with_no_shared_edge_is_placed_again() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let ctx = &nodes[0].engine.ctx;
        let beta = nodes[1].engine.computer_id();
        let mut core = ctx.core.lock().unwrap();
        // An earlier version left a gap between Alpha (0..527) and Beta.
        let desk = core.state.desk.as_mut().unwrap();
        for p in &mut desk.state.layout.placements {
            if p.display_id.computer == beta {
                p.x = 530;
            }
        }
        let epoch = desk.state.epoch;
        ctx.sync_layout(&mut core);
        let state = &core.state.desk.as_ref().unwrap().state;
        let mut at: Vec<_> = state.layout.placements.iter().map(|p| p.x).collect();
        at.sort();
        assert_eq!(at, [0, 527]);
        assert_eq!(state.epoch, epoch + 1);
        // Once repaired, nothing changes again.
        ctx.sync_layout(&mut core);
        assert_eq!(core.state.desk.as_ref().unwrap().state.epoch, epoch + 1);
    }

    #[tokio::test]
    async fn placing_waits_for_the_size_of_every_placed_display() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let ctx = &a.engine.ctx;
        let beta = b.engine.computer_id();
        let mut core = ctx.core.lock().unwrap();
        // Alpha lost its own placement and forgot Beta's displays: placing
        // anything now could overlap Beta's.
        let known = core.state.displays.remove(&beta).unwrap();
        let desk = core.state.desk.as_mut().unwrap();
        let alphas = desk
            .state
            .layout
            .placements
            .iter()
            .find(|p| p.display_id.computer != beta)
            .unwrap()
            .display_id
            .clone();
        desk.state
            .layout
            .placements
            .retain(|p| p.display_id.computer == beta);
        desk.state
            .layout
            .edge_options
            .push(mouser_proto::model::EdgeOptions::new(
                alphas,
                mouser_proto::model::Side::Right,
            ));
        let before = desk.state.clone();
        ctx.sync_layout(&mut core);
        assert_eq!(core.state.desk.as_ref().unwrap().state, before);
        // Once Beta's displays are known again, it goes to the right of them.
        core.state.displays.insert(beta, known);
        ctx.sync_layout(&mut core);
        let layout = &core.state.desk.as_ref().unwrap().state.layout;
        assert!(
            layout.edge_options.is_empty(),
            "replaced displays lose their options"
        );
        let mut at: Vec<_> = layout.placements.iter().map(|p| p.x).collect();
        at.sort();
        assert_eq!(at, [527, 1054]);
    }
}
