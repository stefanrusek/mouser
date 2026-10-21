use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId, Layout, Placement};

use super::validate::validate;

fn display(computer: u8, key: &str) -> Display {
    Display {
        display_id: DisplayId {
            computer: ComputerId([computer; 16]),
            key: key.into(),
        },
        name: key.into(),
        pixel_size: (1000, 1000),
        logical_size: (1000.0, 1000.0),
        scale: 1.0,
        physical_size_mm: Some((100.0, 100.0)),
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

#[test]
fn side_by_side_and_stacked_displays_are_valid() {
    let (a, b, c) = (display(1, "a"), display(2, "b"), display(2, "c"));
    let all = [a.clone(), b.clone(), c.clone()];
    assert_eq!(
        validate(&all, &layout(&[(&a, 0, 0), (&b, 100, 0), (&c, 100, 100)])),
        Ok(())
    );
    // A single display, and none at all, are valid.
    assert_eq!(
        validate(std::slice::from_ref(&a), &layout(&[(&a, 7, 7)])),
        Ok(())
    );
    assert_eq!(validate(&[], &Layout::default()), Ok(()));
}

#[test]
fn overlapping_displays_are_refused() {
    let (a, b) = (display(1, "a"), display(2, "b"));
    let all = [a.clone(), b.clone()];
    let error = validate(&all, &layout(&[(&a, 0, 0), (&b, 50, 50)])).unwrap_err();
    assert_eq!(error, "a and b overlap");
}

#[test]
fn a_display_that_only_touches_at_a_corner_is_cut_off() {
    let (a, b) = (display(1, "a"), display(2, "b"));
    let all = [a.clone(), b.clone()];
    let error = validate(&all, &layout(&[(&a, 0, 0), (&b, 100, 100)])).unwrap_err();
    assert_eq!(error, "b cannot be reached from the other displays");
    // A gap cuts it off too.
    let error = validate(&all, &layout(&[(&a, 0, 0), (&b, 300, 0)])).unwrap_err();
    assert_eq!(error, "b cannot be reached from the other displays");
}

#[test]
fn an_included_display_without_a_placement_is_refused() {
    let (a, b) = (display(1, "a"), display(2, "b"));
    let all = [a.clone(), b];
    let error = validate(&all, &layout(&[(&a, 0, 0)])).unwrap_err();
    assert_eq!(error, "b has no place on the desk");
}

#[test]
fn a_placement_of_a_removed_or_unknown_display_is_refused() {
    let (a, mut b, c) = (display(1, "a"), display(2, "b"), display(3, "c"));
    b.included = false;
    let all = [a.clone(), b.clone()];
    let error = validate(&all, &layout(&[(&a, 0, 0), (&b, 100, 0)])).unwrap_err();
    assert_eq!(error, "b is not part of the layout");
    let error = validate(&all, &layout(&[(&a, 0, 0), (&c, 100, 0)])).unwrap_err();
    assert_eq!(error, "c is not a display on this desk");
    // Without the removed display's placement it is valid.
    assert_eq!(validate(&all, &layout(&[(&a, 0, 0)])), Ok(()));
}
