//! Layout validation (data-model.md "Layout"): placements of included
//! displays must not overlap and every included display must be reachable.

use mouser_proto::model::{Display, Layout};

use super::autoplace;
use super::edges;

/// True when the included `displays` of one computer are placed in `layout`
/// in the arrangement the OS reports, moved as one: every placement lies the
/// same distance from where the arrangement puts that display.
pub fn rigid(displays: &[Display], layout: &Layout) -> bool {
    let included: Vec<Display> = displays.iter().filter(|d| d.included).cloned().collect();
    let mut shifts = autoplace::arrange(&included).into_iter().map(|(d, x, y)| {
        let at = layout
            .placements
            .iter()
            .find(|p| p.display_id == d.display_id);
        at.map(|p| (p.x - x, p.y - y))
    });
    let first = shifts.next().flatten();
    shifts.all(|shift| shift == first)
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
    let computers: std::collections::BTreeSet<_> =
        displays.iter().map(|d| d.display_id.computer).collect();
    for computer in computers {
        let own: Vec<Display> = displays
            .iter()
            .filter(|d| d.display_id.computer == computer)
            .cloned()
            .collect();
        if !rigid(&own, layout) {
            return Err(format!(
                "The displays of {} keep the arrangement of that computer's own settings; move the computer as a whole",
                own[0].name
            ));
        }
    }
    let placed = edges::placed(displays, layout);
    for (i, a) in placed.iter().enumerate() {
        if let Some(b) = placed[i + 1..]
            .iter()
            .find(|b| edges::overlap(&a.rect, &b.rect))
        {
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
