use super::capture::*;
use super::portal::Zone;
use mouser_platform::LocalInput;

fn zone(x: i32, y: i32, width: u32, height: u32) -> Zone {
    Zone {
        x,
        y,
        width,
        height,
    }
}

fn edge(side: Side, line: (i32, i32, i32, i32)) -> Edge {
    Edge { side, line }
}

#[test]
fn a_single_display_has_a_barrier_on_each_side() {
    assert_eq!(
        outer_edges(&[zone(0, 0, 1920, 1080)]),
        [
            edge(Side::Left, (0, 0, 0, 1079)),
            edge(Side::Right, (1920, 0, 1920, 1079)),
            edge(Side::Top, (0, 0, 1919, 0)),
            edge(Side::Bottom, (0, 1080, 1919, 1080)),
        ]
    );
}

#[test]
fn side_by_side_displays_have_no_barrier_between_them() {
    let edges = outer_edges(&[zone(0, 0, 1000, 800), zone(1000, 0, 1000, 800)]);
    assert_eq!(
        edges,
        [
            edge(Side::Left, (0, 0, 0, 799)),
            edge(Side::Top, (0, 0, 999, 0)),
            edge(Side::Bottom, (0, 800, 999, 800)),
            edge(Side::Right, (2000, 0, 2000, 799)),
            edge(Side::Top, (1000, 0, 1999, 0)),
            edge(Side::Bottom, (1000, 800, 1999, 800)),
        ]
    );
}

#[test]
fn a_shorter_neighbour_leaves_the_rest_of_the_shared_edge_guarded() {
    let edges = outer_edges(&[zone(0, 0, 1000, 800), zone(1000, 200, 500, 400)]);
    let right_of_first: Vec<_> = edges
        .iter()
        .filter(|e| e.side == Side::Right && e.line.0 == 1000)
        .collect();
    assert_eq!(
        right_of_first,
        [
            &edge(Side::Right, (1000, 0, 1000, 199)),
            &edge(Side::Right, (1000, 600, 1000, 799)),
        ]
    );
    // The shorter display's left edge is fully against the first one.
    assert!(
        !edges
            .iter()
            .any(|e| e.side == Side::Left && e.line.0 == 1000)
    );
}

#[test]
fn displays_that_only_touch_at_a_corner_keep_all_their_edges() {
    let edges = outer_edges(&[zone(0, 0, 100, 100), zone(100, 100, 100, 100)]);
    assert_eq!(edges.len(), 8);
}

#[test]
fn pushing_outward_is_a_unit_step_in_the_direction_of_the_side() {
    let steps: Vec<_> = [Side::Left, Side::Right, Side::Top, Side::Bottom]
        .into_iter()
        .map(|side| side.outward())
        .collect();
    assert_eq!(steps, [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]);
}

#[test]
fn linux_button_codes_map_to_mouser_numbering() {
    assert_eq!(
        [0x110, 0x111, 0x112, 0x113, 0x114].map(mouser_button),
        [Some(1), Some(2), Some(3), Some(4), Some(5)]
    );
    assert_eq!(mouser_button(0x115), None);
}

#[test]
fn scroll_is_pixels_positive_up_and_right() {
    assert_eq!(
        scroll_delta(2.0, 10.0),
        LocalInput::Scroll { dx: 2.0, dy: -10.0 }
    );
    assert_eq!(
        scroll_clicks(120, -240),
        LocalInput::Scroll { dx: 15.0, dy: 30.0 }
    );
}
