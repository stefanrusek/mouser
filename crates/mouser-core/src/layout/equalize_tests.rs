use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId, Layout, Placement};

use super::edges::{self, Placed, Step};
use super::equalize::step;

fn display(computer: u8, key: &str, mm: (f32, f32)) -> Display {
    Display {
        display_id: DisplayId {
            computer: ComputerId([computer; 16]),
            key: key.into(),
        },
        name: key.into(),
        pixel_size: (1000, 1000),
        logical_size: (1000.0, 1000.0),
        scale: 1.0,
        physical_size_mm: Some(mm),
        origin_local: (0, 0),
        included: true,
    }
}

fn desk(at: &[(&Display, i32, i32)]) -> Vec<Placed> {
    let layout = Layout {
        placements: at
            .iter()
            .map(|(d, x, y)| Placement {
                display_id: d.display_id.clone(),
                x: *x,
                y: *y,
            })
            .collect(),
        ..Layout::default()
    };
    let displays: Vec<_> = at.iter().map(|(d, ..)| (*d).clone()).collect();
    edges::placed(&displays, &layout)
}

/// a is 100x100 at the origin; b, right of it, is 100x50.
fn short_neighbour() -> Vec<Placed> {
    let (a, b) = (
        display(1, "a", (100.0, 100.0)),
        display(2, "b", (100.0, 50.0)),
    );
    desk(&[(&a, 0, 0), (&b, 100, 0)])
}

fn crossed(step: Step) -> (usize, (f64, f64)) {
    let Step::Cross { to, at } = step else {
        panic!("expected a crossing, got {step:?}");
    };
    (to, at)
}

#[test]
fn off_it_is_the_ordinary_mapping() {
    let p = short_neighbour();
    let (to, at) = crossed(step(&p, 0, (101.0, 25.0), false));
    assert_eq!((to, at), (1, (101.0, 25.0)));
    // Beyond the overlap there is no edge.
    assert!(matches!(
        step(&p, 0, (101.0, 75.0), false),
        Step::Clamp { .. }
    ));
}

#[test]
fn on_the_full_length_of_one_edge_maps_onto_the_full_length_of_the_other() {
    let p = short_neighbour();
    // Halfway down a (50 of 100) is halfway down b (25 of 50).
    let (to, at) = crossed(step(&p, 0, (101.0, 50.0), true));
    assert_eq!(to, 1);
    assert!((at.1 - 25.0).abs() < 1e-9, "{at:?}");
    // The bottom of a reaches the bottom of b, where no edge existed.
    let (_, at) = crossed(step(&p, 0, (101.0, 99.9), true));
    assert!((at.1 - 49.95).abs() < 1e-6 && at.1 < 50.0, "{at:?}");
    // And back: the whole of b's edge is the whole of a's.
    let (to, at) = crossed(step(&p, 1, (99.0, 25.0), true));
    assert_eq!(to, 0);
    assert!((at.1 - 50.0).abs() < 1e-9 && at.0 < 100.0, "{at:?}");
    // The top maps to the top.
    let (_, at) = crossed(step(&p, 0, (101.0, 0.0), true));
    assert_eq!(at.1, 0.0);
}

#[test]
fn equalizing_works_along_the_horizontal_edges_too() {
    let (a, b) = (
        display(1, "a", (100.0, 100.0)),
        display(2, "b", (50.0, 100.0)),
    );
    let p = desk(&[(&a, 0, 0), (&b, 0, 100)]);
    let (to, at) = crossed(step(&p, 0, (80.0, 101.0), true));
    assert_eq!(to, 1);
    assert!((at.0 - 40.0).abs() < 1e-9, "{at:?}");
    let (to, at) = crossed(step(&p, 1, (10.0, 99.0), true));
    assert_eq!(to, 0);
    assert!((at.0 - 20.0).abs() < 1e-9, "{at:?}");
}

#[test]
fn staying_and_walls_are_unchanged_and_the_nearest_neighbour_takes_a_stray_point() {
    let p = short_neighbour();
    assert_eq!(step(&p, 0, (10.0, 10.0), true), Step::Stay);
    // a's left has no neighbour.
    assert!(matches!(
        step(&p, 0, (-1.0, 10.0), true),
        Step::Clamp { .. }
    ));
    // Past the corner of a's edge, the neighbour is still b.
    let (to, _) = crossed(step(&p, 0, (101.0, 120.0), true));
    assert_eq!(to, 1);
}

#[test]
fn with_several_neighbours_on_a_side_the_one_the_pointer_left_toward_takes_it() {
    let a = display(1, "a", (100.0, 100.0));
    let (b, c) = (
        display(2, "b", (100.0, 50.0)),
        display(3, "c", (100.0, 50.0)),
    );
    let p = desk(&[(&a, 0, 0), (&b, 100, 0), (&c, 100, 50)]);
    let (to, at) = crossed(step(&p, 0, (101.0, 25.0), true));
    assert_eq!(to, 1);
    assert!((at.1 - 12.5).abs() < 1e-9, "{at:?}");
    let (to, at) = crossed(step(&p, 0, (101.0, 75.0), true));
    assert_eq!(to, 2);
    assert!((at.1 - 87.5).abs() < 1e-9, "{at:?}");
    // Past the end of a's edge, the nearest neighbour.
    let (to, _) = crossed(step(&p, 0, (101.0, 120.0), true));
    assert_eq!(to, 2);
}
