//! Wayland displays from the InputCapture zones.

use super::capture::SharedZones;
use super::portal::{Zone, start_capture, zones};
use crate::bus::{block_on, connect};
use mouser_platform::LocalDisplay;
use std::collections::HashMap;

/// The desktop reports zones in logical pixels without scale or physical
/// size, so each display has scale 1.0 and no physical size.
pub(crate) fn display(index: usize, zone: &Zone) -> LocalDisplay {
    let (width, height) = (zone.width, zone.height);
    LocalDisplay {
        key: format!("zone-{index}"),
        name: format!("Display {}", index + 1),
        pixel_size: (width, height),
        logical_size: (f64::from(width), f64::from(height)),
        scale: 1.0,
        physical_size_mm: None,
        origin: (f64::from(zone.x), f64::from(zone.y)),
    }
}

/// The zones of the running capture, or else those of a short-lived capture
/// session (which may ask the user to approve Mouser).
pub(crate) fn displays(env: &HashMap<String, String>, shared: &SharedZones) -> Vec<LocalDisplay> {
    let known = shared.lock().unwrap().clone();
    let zones = known.unwrap_or_else(|| {
        let env = env.clone();
        let zones = block_on(async move {
            let conn = connect(&env).await.expect("session bus");
            let capture = start_capture(conn, None).await;
            let (zones, _) = zones(&capture).await;
            capture.session.close().await.unwrap();
            zones
        });
        *shared.lock().unwrap() = Some(zones.clone());
        zones
    });
    zones
        .iter()
        .enumerate()
        .map(|(i, z)| display(i, z))
        .collect()
}
