//! Where on computer A a push to the right must start to cross onto computer B,
//! read from the desk layout Mouser saved on A (`state.json`, next to the
//! `logs` folder that `mouser status` names).
//!
//! A push only crosses if one of its mouse-moved events is located on the A
//! display whose right edge borders B, within one step of that edge: Mouser
//! looks up the display under the event's location, and an event located past
//! every display of A (the pointer can be left there by posted events) crosses
//! nothing. So the push starts inside that display, a little left of its right
//! edge, on the row of the middle of the shared edge.

use serde_json::Value;

/// How far left of the bordering edge the push starts, in points.
const LEAD_IN: f64 = 200.0;

const MM_PER_96DPI_PX: f64 = 25.4 / 96.0;

/// A display of the desk with its rectangle in desk millimetres, as Mouser
/// places it (crates/mouser-core/src/layout/edges.rs).
struct Placed<'a> {
    display: &'a Value,
    x: i64,
    y: i64,
    w: i64,
    h: i64,
}

fn num(v: &Value) -> f64 {
    v.as_f64().expect("a number")
}

/// The display's size on the desk: its physical size, or its logical size at
/// 96 dpi, in whole millimetres.
fn desk_size(display: &Value) -> (i64, i64) {
    let mm = &display["physical_size_mm"];
    let logical = &display["logical_size"];
    let (w, h) = if mm.is_array() {
        (num(&mm[0]), num(&mm[1]))
    } else {
        (
            f64::from((num(&logical[0]) * MM_PER_96DPI_PX) as f32),
            f64::from((num(&logical[1]) * MM_PER_96DPI_PX) as f32),
        )
    };
    (w.round() as i64, h.round() as i64)
}

/// The included displays of `computer` that the layout places.
fn placed<'a>(state: &'a Value, computer: &str) -> Vec<Placed<'a>> {
    let placements = state["desk"]["state"]["layout"]["placements"]
        .as_array()
        .expect("the saved desk has a layout");
    state["displays"][computer]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter(|d| d["included"] == true)
        .filter_map(|d| {
            let at = placements
                .iter()
                .find(|p| p["display_id"] == d["display_id"])?;
            let (w, h) = desk_size(d);
            Some(Placed {
                display: d,
                x: at["x"].as_i64().expect("x"),
                y: at["y"].as_i64().expect("y"),
                w,
                h,
            })
        })
        .collect()
}

/// The point, in `from`'s desktop coordinates, where a rightward push must
/// start to cross onto `onto`: inside the display of `from` whose right edge
/// borders a display of `onto`, `LEAD_IN` points left of that edge, on the
/// row of the middle of the shared stretch. `None` when no display of `from`
/// borders `onto` on the right.
pub fn push_start(state: &Value, from: &str, onto: &str) -> Option<(f64, f64)> {
    let targets = placed(state, onto);
    placed(state, from).into_iter().find_map(|a| {
        let b = targets
            .iter()
            .find(|b| a.x + a.w == b.x && a.y.max(b.y) < (a.y + a.h).min(b.y + b.h))?;
        let middle = (a.y.max(b.y) + (a.y + a.h).min(b.y + b.h)) as f64 / 2.0;
        let origin = &a.display["origin_local"];
        let (w, h) = (
            num(&a.display["logical_size"][0]),
            num(&a.display["logical_size"][1]),
        );
        Some((
            num(&origin[0]) + (w - LEAD_IN).max(w / 2.0),
            num(&origin[1]) + (middle - a.y as f64) * h / a.h as f64,
        ))
    })
}
