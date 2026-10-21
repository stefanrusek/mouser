//! Display exchange: a member's local displays as wire `Display`s.

use mouser_platform::LocalDisplay;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{Display, DisplayId};

/// The wire form of `displays`, all included in the layout.
pub fn to_wire(computer: ComputerId, displays: &[LocalDisplay]) -> Vec<Display> {
    displays
        .iter()
        .map(|d| Display {
            display_id: DisplayId {
                computer,
                key: d.key.clone(),
            },
            name: d.name.clone(),
            pixel_size: d.pixel_size,
            logical_size: (d.logical_size.0 as f32, d.logical_size.1 as f32),
            scale: d.scale,
            physical_size_mm: d.physical_size_mm.map(|(w, h)| (w as f32, h as f32)),
            origin_local: (d.origin.0 as i32, d.origin.1 as i32),
            included: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_displays_map_to_wire_displays() {
        let local = LocalDisplay {
            key: "d1".into(),
            name: "Studio Display".into(),
            pixel_size: (5120, 2880),
            logical_size: (2560.0, 1440.0),
            scale: 2.0,
            physical_size_mm: Some((597.0, 336.0)),
            origin: (-2560.0, 10.0),
        };
        let wire = to_wire(ComputerId([4; 16]), &[local]);
        assert_eq!(wire.len(), 1);
        let d = &wire[0];
        assert_eq!(d.display_id.computer, ComputerId([4; 16]));
        assert_eq!(d.display_id.key, "d1");
        assert_eq!(d.pixel_size, (5120, 2880));
        assert_eq!(d.logical_size, (2560.0, 1440.0));
        assert_eq!(d.scale, 2.0);
        assert_eq!(d.physical_size_mm, Some((597.0, 336.0)));
        assert_eq!(d.origin_local, (-2560, 10));
        assert!(d.included);
    }
}
