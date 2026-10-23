//! Unit tests of `layout.rs`, run by the `e2e_steps` test target.

use crate::layout::push_start;
use serde_json::{Value, json};

const A: &str = "9e4ac5618a56f17190c2af3dac2a571c";
const B: &str = "aed223966d20022ffb14ad52ff18fb4f";

fn display(computer: &str, key: &str, logical: (f64, f64), mm: Value, origin: (i64, i64)) -> Value {
    json!({
        "display_id": {"computer": computer, "key": key},
        "logical_size": [logical.0, logical.1],
        "physical_size_mm": mm,
        "origin_local": [origin.0, origin.1],
        "included": true
    })
}

fn place(computer: &str, key: &str, x: i64, y: i64) -> Value {
    json!({"display_id": {"computer": computer, "key": key}, "x": x, "y": y})
}

/// The desk J1 formed: A's main display (2560x1440 pt) with a 3440x1440
/// display above it whose right edge is at the same local x, and B's
/// single display placed against the main display's right edge.
fn desk(placements: Vec<Value>) -> Value {
    json!({
        "desk": {"state": {"layout": {"placements": placements}}},
        "displays": {
            A: [
                display(A, "main", (2560.0, 1440.0), json!([599.29956, 340.24185]), (0, 0)),
                display(A, "wide", (3440.0, 1440.0), json!([801.6147, 338.66666]), (-880, -1440)),
            ],
            B: [display(B, "vm", (1474.0, 979.0), json!([340.0, 220.0]), (0, 0))],
        }
    })
}

#[test]
fn starts_left_of_the_edge_that_borders_b_on_the_middle_of_the_shared_stretch() {
    let state = desk(vec![
        place(A, "main", 205, 339),
        place(A, "wide", 0, 0),
        place(B, "vm", 804, 339),
    ]);
    let (x, y) = push_start(&state, A, B).expect("A borders B");
    assert_eq!(x, 2360.0);
    // Shared stretch 339..559 mm, middle 449 mm: 110 mm into a 340 mm tall display.
    assert!((y - 110.0 * 1440.0 / 340.0).abs() < 1e-9, "{y}");
}

#[test]
fn picks_the_display_that_borders_b_even_when_it_is_not_the_main_one() {
    // B against the wide display's right edge (0 + 802 mm), top-aligned.
    let state = desk(vec![
        place(A, "main", 205, 339),
        place(A, "wide", 0, 0),
        place(B, "vm", 802, 0),
    ]);
    let (x, y) = push_start(&state, A, B).expect("A borders B");
    assert_eq!(x, -880.0 + 3440.0 - 200.0);
    // Shared stretch 0..220 mm of the 339 mm tall wide display.
    assert!((y - (-1440.0 + 110.0 * 1440.0 / 339.0)).abs() < 1e-9, "{y}");
}

#[test]
fn none_when_no_display_of_a_borders_b_on_the_right() {
    let state = desk(vec![
        place(A, "main", 205, 339),
        place(A, "wide", 0, 0),
        place(B, "vm", 0, 679),
    ]);
    assert_eq!(push_start(&state, A, B), None);
}

#[test]
fn displays_without_a_physical_size_are_sized_at_96_dpi_and_narrow_ones_start_in_the_middle() {
    // 96 x 960 pt at 96 dpi is 25 x 254 mm; 200 pt of lead-in does not fit.
    let state = json!({
        "desk": {"state": {"layout": {"placements": [
            place(A, "small", 0, 0),
            place(B, "vm", 25, 0),
        ]}}},
        "displays": {
            A: [display(A, "small", (96.0, 960.0), Value::Null, (0, 0))],
            B: [display(B, "vm", (1474.0, 979.0), json!([340.0, 220.0]), (0, 0))],
        }
    });
    let (x, y) = push_start(&state, A, B).expect("A borders B");
    assert_eq!(x, 48.0);
    assert!((y - 110.0 * 960.0 / 254.0).abs() < 1e-9, "{y}");
}
