use super::displays::display;
use super::portal::Zone;

#[test]
fn a_zone_is_a_display_in_logical_pixels() {
    let d = display(
        1,
        &Zone {
            x: 1920,
            y: -56,
            width: 1280,
            height: 1024,
        },
    );
    assert_eq!(d.key, "zone-1");
    assert_eq!(d.name, "Display 2");
    assert_eq!(d.pixel_size, (1280, 1024));
    assert_eq!(d.logical_size, (1280.0, 1024.0));
    assert_eq!(d.scale, 1.0);
    assert_eq!(d.physical_size_mm, None);
    assert_eq!(d.origin, (1920.0, -56.0));
}
