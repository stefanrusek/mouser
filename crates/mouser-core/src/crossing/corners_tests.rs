use mouser_platform::LocalInput;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId, Layout, Placement, Side};
use mouser_testkit::Injected;

use super::corners::{CORNER_PX, in_corner};
use crate::layout::edges;
use crate::testing::*;

/// 1000x500 logical pixels on 100x50 mm: 10 px per mm.
fn wide() -> edges::Placed {
    let display = Display {
        display_id: DisplayId {
            computer: ComputerId([1; 16]),
            key: "w".into(),
        },
        name: "w".into(),
        pixel_size: (1000, 500),
        logical_size: (1000.0, 500.0),
        scale: 1.0,
        physical_size_mm: Some((100.0, 50.0)),
        origin_local: (0, 0),
        included: true,
    };
    let layout = Layout {
        placements: vec![Placement {
            display_id: display.display_id.clone(),
            x: 0,
            y: 0,
        }],
        ..Layout::default()
    };
    edges::placed(&[display], &layout).remove(0)
}

#[test]
fn the_corner_is_a_fixed_number_of_pixels_at_either_end_of_the_edge() {
    let p = wide();
    let mm = CORNER_PX / 10.0;
    // Vertical edges are 50 mm long.
    assert!(in_corner(&p, Side::Right, mm - 0.01));
    assert!(!in_corner(&p, Side::Right, mm + 0.01));
    assert!(!in_corner(&p, Side::Left, 50.0 - mm - 0.01));
    assert!(in_corner(&p, Side::Left, 50.0 - mm + 0.01));
    // Horizontal edges are 100 mm long.
    assert!(in_corner(&p, Side::Top, mm - 0.01));
    assert!(!in_corner(&p, Side::Top, 50.0));
    assert!(in_corner(&p, Side::Bottom, 100.0 - mm + 0.01));
}

fn push(node: &TestNode, abs: (f64, f64)) {
    node.platform.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs,
    });
}

#[tokio::test]
async fn pushing_into_either_corner_of_a_shared_edge_does_not_cross() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    for y in [1.0, 1078.0] {
        push(a, (1919.0, y));
        // The input thread handles events in order: a crossing at the
        // middle shows the corner pushes before it were decided.
    }
    push_right(a);
    injected_where(&b.platform, "the crossing mid-edge", |i| {
        matches!(i, Injected::MotionAbs { .. })
    })
    .await;
    assert_eq!(
        b.platform
            .injected()
            .iter()
            .filter(|i| matches!(i, Injected::MotionAbs { .. }))
            .count(),
        1,
        "only the mid-edge push crossed"
    );
}
