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
fn new_displays_go_right_of_the_bounding_box_top_aligned() {
    // Two kept displays: 0..527 and 600..1100, tops at 10 and 40.
    let kept = [kept(0, 10, 527, 296), kept(600, 40, 500, 300)];
    let placed = place(&kept, &[display(2, "b", (-100, 50))]);
    assert_eq!(at(&placed, "b"), (1100, 10));
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
