use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId, Layout, Placement, Side};

use super::edges::*;

fn display(computer: u8, key: &str, logical: (f32, f32), mm: Option<(f32, f32)>) -> Display {
    Display {
        display_id: DisplayId {
            computer: ComputerId([computer; 16]),
            key: key.into(),
        },
        name: key.into(),
        pixel_size: (logical.0 as u32, logical.1 as u32),
        logical_size: logical,
        scale: 1.0,
        physical_size_mm: mm,
        origin_local: (0, 0),
        included: true,
    }
}

fn layout(at: &[(&Display, i32, i32)]) -> Layout {
    Layout {
        placements: at
            .iter()
            .map(|(d, x, y)| Placement {
                display_id: d.display_id.clone(),
                x: *x,
                y: *y,
            })
            .collect(),
        ..Layout::default()
    }
}

/// a (0,0 527x296) | b (527,0 527x296), c below b, d far away.
fn four() -> Vec<Placed> {
    let a = display(1, "a", (1920.0, 1080.0), Some((527.0, 296.0)));
    let b = display(2, "b", (1920.0, 1080.0), Some((527.0, 296.0)));
    let c = display(2, "c", (1280.0, 720.0), Some((300.0, 200.0)));
    let d = display(3, "d", (100.0, 100.0), Some((50.0, 50.0)));
    let l = layout(&[(&a, 0, 0), (&b, 527, 0), (&c, 600, 296), (&d, 5000, 5000)]);
    placed(&[a, b, c, d], &l)
}

#[test]
fn size_is_physical_millimetres_else_96_dpi_pixels() {
    let known = display(1, "a", (1920.0, 1080.0), Some((527.4, 296.6)));
    assert_eq!(desk_size(&known), (527, 297));
    let unknown = display(1, "a", (960.0, 540.0), None);
    assert_eq!(desk_size(&unknown), (254, 143));
}

#[test]
fn only_included_displays_with_a_placement_are_placed() {
    let a = display(1, "a", (100.0, 100.0), Some((10.0, 10.0)));
    let mut b = display(1, "b", (100.0, 100.0), Some((10.0, 10.0)));
    let c = display(1, "c", (100.0, 100.0), Some((10.0, 10.0)));
    b.included = false;
    let l = layout(&[(&a, 3, 4), (&b, 20, 0)]);
    let p = placed(&[a, b, c], &l);
    assert_eq!(p.len(), 1);
    assert_eq!(
        p[0].rect,
        Rect {
            x: 3,
            y: 4,
            w: 10,
            h: 10
        }
    );
    assert_eq!((p[0].rect.right(), p[0].rect.bottom()), (13, 14));
}

#[test]
fn edges_are_shared_boundary_segments_in_both_directions() {
    let p = four();
    let e = edges(&p);
    assert!(e.contains(&Edge {
        from: 0,
        side: Side::Right,
        to: 1,
        lo: 0,
        hi: 296
    }));
    assert!(e.contains(&Edge {
        from: 1,
        side: Side::Left,
        to: 0,
        lo: 0,
        hi: 296
    }));
    // c hangs under b (x 600..900 against 527..1054): shared 600..900.
    assert!(e.contains(&Edge {
        from: 1,
        side: Side::Bottom,
        to: 2,
        lo: 600,
        hi: 900
    }));
    assert!(e.contains(&Edge {
        from: 2,
        side: Side::Top,
        to: 1,
        lo: 600,
        hi: 900
    }));
    assert_eq!(e.len(), 4, "d touches nothing and a/c only meet at a point");
}

#[test]
fn touching_only_at_a_corner_is_not_an_edge() {
    let a = display(1, "a", (10.0, 10.0), Some((10.0, 10.0)));
    let b = display(2, "b", (10.0, 10.0), Some((10.0, 10.0)));
    let p = placed(
        &[a.clone(), b.clone()],
        &layout(&[(&a, 0, 0), (&b, 10, 10)]),
    );
    assert!(edges(&p).is_empty());
}

#[test]
fn points_map_between_desk_space_and_display_pixels() {
    let p = four();
    assert_eq!(to_desk(&p[1], (960.0, 540.0)), (527.0 + 263.5, 148.0));
    assert_eq!(to_local(&p[1], (527.0 + 263.5, 148.0)), (960.0, 540.0));
    // Kept on the display.
    assert_eq!(to_local(&p[1], (-10.0, 9999.0)), (0.0, 1079.0));
    // Different densities: 296 mm is 1080 px on b but 200 mm is 720 px on c.
    let (mx, my) = mm_per_px(&p[2]);
    assert!((mx - 300.0 / 1280.0).abs() < 1e-12 && (my - 200.0 / 720.0).abs() < 1e-12);
}

#[test]
fn leaving_through_an_adjoining_edge_lands_at_the_matching_position() {
    let p = four();
    // Past a's right edge at half height: onto b, same height in mm.
    let Step::Cross { to, at } = step(&p, 0, (528.5, 148.0)) else {
        panic!("expected a crossing");
    };
    assert_eq!(to, 1);
    assert_eq!(at, (528.5, 148.0));
    assert_eq!(to_local(&p[to], at).1, 540.0);
    // Back from b's left edge.
    let Step::Cross { to, .. } = step(&p, 1, (526.0, 10.0)) else {
        panic!("expected a crossing");
    };
    assert_eq!(to, 0);
    // Down from b into c, and up again.
    let Step::Cross { to, .. } = step(&p, 1, (700.0, 296.5)) else {
        panic!("expected a crossing");
    };
    assert_eq!(to, 2);
    let Step::Cross { to, .. } = step(&p, 2, (700.0, 295.0)) else {
        panic!("expected a crossing");
    };
    assert_eq!(to, 1);
}

#[test]
fn staying_and_hitting_a_wall() {
    let p = four();
    assert_eq!(step(&p, 0, (10.0, 10.0)), Step::Stay);
    // a's top, left and bottom have no neighbour; so does b's right edge.
    for (from, at) in [
        (0, (10.0, -5.0)),
        (0, (-5.0, 10.0)),
        (0, (10.0, 400.0)),
        (1, (2000.0, 10.0)),
    ] {
        let Step::Clamp { at: held } = step(&p, from, at) else {
            panic!("expected a wall at {at:?}");
        };
        assert!(p[from].rect.contains(held.0, held.1), "{held:?}");
    }
    // The edge only covers where the displays overlap: below a's range on b's side.
    assert!(matches!(
        step(&p, 1, (527.0 - 1.0, 300.0)),
        Step::Clamp { .. }
    ));
}

#[test]
fn a_display_is_found_by_id() {
    let p = four();
    assert_eq!(index_of(&p, &p[2].display.display_id), Some(2));
    let missing = DisplayId {
        computer: ComputerId([9; 16]),
        key: "x".into(),
    };
    assert_eq!(index_of(&p, &missing), None);
}
