//! Layout validation (data-model.md "Layout"): placements of included
//! displays must not overlap and every included display must be reachable.

use mouser_proto::model::{Display, Layout};

use super::edges::{self, Placed};

fn overlap(a: &Placed, b: &Placed) -> bool {
    a.rect.x < b.rect.right()
        && b.rect.x < a.rect.right()
        && a.rect.y < b.rect.bottom()
        && b.rect.y < a.rect.bottom()
}

/// `Ok` when `layout` is a valid placement of the included displays among
/// `displays`; otherwise a sentence saying what is wrong.
pub fn validate(displays: &[Display], layout: &Layout) -> Result<(), String> {
    for p in &layout.placements {
        let Some(d) = displays.iter().find(|d| d.display_id == p.display_id) else {
            return Err(format!(
                "{} is not a display on this desk",
                p.display_id.key
            ));
        };
        if !d.included {
            return Err(format!("{} is not part of the layout", d.name));
        }
    }
    if let Some(d) = displays.iter().find(|d| {
        d.included
            && !layout
                .placements
                .iter()
                .any(|p| p.display_id == d.display_id)
    }) {
        return Err(format!("{} has no place on the desk", d.name));
    }
    let placed = edges::placed(displays, layout);
    for (i, a) in placed.iter().enumerate() {
        if let Some(b) = placed[i + 1..].iter().find(|b| overlap(a, b)) {
            return Err(format!("{} and {} overlap", a.display.name, b.display.name));
        }
    }
    let mut reached = vec![false; placed.len()];
    let mut queue = Vec::new();
    if !placed.is_empty() {
        reached[0] = true;
        queue.push(0);
    }
    let adjoining = edges::edges(&placed);
    while let Some(i) = queue.pop() {
        for e in adjoining.iter().filter(|e| e.from == i) {
            if !reached[e.to] {
                reached[e.to] = true;
                queue.push(e.to);
            }
        }
    }
    match reached.iter().position(|r| !r) {
        Some(i) => Err(format!(
            "{} cannot be reached from the other displays",
            placed[i].display.name
        )),
        None => Ok(()),
    }
}
