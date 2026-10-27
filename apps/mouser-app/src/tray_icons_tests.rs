use tauri::image::Image;

use crate::tray::{Appearance, Icon};
use mouser_platform::Os;
use mouser_proto::control::TrayConnection;

const THEMES: [&str; 3] = ["light", "dark", "template"];
const STATES: [&str; 2] = ["active", "paused"];
const CONNECTIONS: [&str; 3] = ["connected", "disconnected", "alone"];

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
fn in_greyscale_the_three_connections_of_each_state_differ() {
    for theme in THEMES {
        for state in STATES {
            let [connected, disconnected, alone] =
                CONNECTIONS.map(|c| greyscale(&file(theme, state, c)));
            assert!(connected != disconnected, "{theme} {state}");
            assert!(connected != alone, "{theme} {state}");
            assert!(disconnected != alone, "{theme} {state}");
        }
    }
}

#[test]
fn the_icon_compiled_in_is_the_file_named_by_its_variant() {
    for (os, appearance, theme) in [
        (Os::MacOs, Appearance::Light, "template"),
        (Os::Windows, Appearance::Light, "light"),
        (Os::Linux, Appearance::Dark, "dark"),
    ] {
        for (paused, state) in [(false, "active"), (true, "paused")] {
            for (connection, name) in [
                (TrayConnection::Connected, "connected"),
                (TrayConnection::Disconnected, "disconnected"),
                (TrayConnection::Alone, "alone"),
            ] {
                let icon = Icon::choose(os, appearance, paused, connection);
                assert_eq!(
                    icon.png(),
                    std::fs::read(file(theme, state, name)).unwrap().as_slice()
                );
            }
        }
    }
}
