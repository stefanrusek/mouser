use tauri::image::Image;

use crate::tray::TrayConnection;
use crate::tray::{Appearance, Icon};

const THEMES: [&str; 2] = ["light", "dark"];
const STATES: [&str; 2] = ["active", "paused"];
const CONNECTIONS: [&str; 4] = ["connected", "partial", "disconnected", "alone"];

fn file(theme: &str, state: &str, connection: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("icons")
        .join(format!("tray-{theme}-{state}-{connection}.png"))
}

fn greyscale(path: &std::path::Path) -> Vec<u8> {
    let image = Image::from_bytes(&std::fs::read(path).expect("the icon file exists"))
        .expect("the icon is a PNG");
    image
        .rgba()
        .chunks(4)
        .map(|p| {
            let grey =
                (u32::from(p[0]) * 299 + u32::from(p[1]) * 587 + u32::from(p[2]) * 114) / 1000;
            // Over a white menu bar, so a black template glyph shows.
            ((grey * u32::from(p[3]) + 255 * (255 - u32::from(p[3]))) / 255) as u8
        })
        .collect()
}

#[test]
fn an_icon_file_exists_for_every_state_connection_and_theme() {
    for theme in THEMES {
        for state in STATES {
            for connection in CONNECTIONS {
                let path = file(theme, state, connection);
                assert!(path.exists(), "{} is missing", path.display());
            }
        }
    }
}

#[test]
fn in_greyscale_the_four_connections_of_each_state_differ() {
    for theme in THEMES {
        for state in STATES {
            let pictures = CONNECTIONS.map(|c| greyscale(&file(theme, state, c)));
            for (i, one) in pictures.iter().enumerate() {
                for other in &pictures[i + 1..] {
                    assert!(one != other, "{theme} {state}");
                }
            }
        }
    }
}

#[test]
fn the_dots_are_green_yellow_and_red_and_alone_has_none() {
    let dot_colour = |connection: &str, x: usize, y: usize| {
        let image = Image::from_bytes(&std::fs::read(file("light", "active", connection)).unwrap())
            .unwrap();
        image.rgba()[(y * 32 + x) * 4..][..4].to_vec()
    };
    // The badge is centred on (24, 24) of the 32x32 icon: the disc and the
    // triangle are solid just below it, the ring is solid to its right.
    assert_eq!(dot_colour("connected", 24, 26), [22, 163, 74, 255]);
    assert_eq!(dot_colour("partial", 24, 26), [234, 179, 8, 255]);
    assert_eq!(dot_colour("disconnected", 29, 24), [217, 45, 32, 255]);
    assert_eq!(dot_colour("alone", 24, 26)[3], 0);
}

#[test]
fn the_icon_compiled_in_is_the_file_named_by_its_variant() {
    for (appearance, theme) in [(Appearance::Light, "light"), (Appearance::Dark, "dark")] {
        for (paused, state) in [(false, "active"), (true, "paused")] {
            for (connection, name) in [
                (TrayConnection::Connected, "connected"),
                (TrayConnection::Partial, "partial"),
                (TrayConnection::Disconnected, "disconnected"),
                (TrayConnection::Alone, "alone"),
            ] {
                let icon = Icon::choose(appearance, paused, connection);
                assert_eq!(
                    icon.png(),
                    std::fs::read(file(theme, state, name)).unwrap().as_slice()
                );
            }
        }
    }
}
