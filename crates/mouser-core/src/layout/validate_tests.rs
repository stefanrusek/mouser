use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId, Layout, Placement};

use super::validate::validate;

fn display_at(computer: u8, key: &str, origin: (i32, i32)) -> Display {
    Display {
        origin_local: origin,
        ..display(computer, key)
    }
}

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
    let (a, b, c) = (
        display(1, "a"),
        display(2, "b"),
        display_at(2, "c", (0, 1000)),
    );
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

#[test]
fn a_computers_displays_must_keep_the_os_arrangement_as_one_group() {
    let (a, b, c) = (
        display(1, "a"),
        display_at(2, "b", (0, 0)),
        display_at(2, "c", (1000, 0)),
    );
    let all = [a.clone(), b.clone(), c.clone()];
    // Moving the pair together is fine.
    assert_eq!(
        validate(&all, &layout(&[(&a, 0, 0), (&b, 100, 50), (&c, 200, 50)])),
        Ok(())
    );
    // Dragging one away from its sibling is refused.
    let err = validate(&all, &layout(&[(&a, 0, 0), (&b, 100, 0), (&c, 200, 100)])).unwrap_err();
    assert!(err.contains("b"), "{err}");
    assert!(err.contains("as a whole"), "{err}");
}
