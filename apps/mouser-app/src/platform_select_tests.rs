use mouser_platform::Os;

use crate::platform_select::{
    Backend, create, current, current_os, linux_backend, process_env, select,
};

fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    move |key| {
        pairs
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| (*v).to_string())
    }
}

#[test]
fn macos_uses_the_macos_backend() {
    assert_eq!(select(Os::MacOs, env(&[])), Backend::MacOs);
}

#[test]
fn windows_uses_the_windows_backend() {
    assert_eq!(select(Os::Windows, env(&[])), Backend::Windows);
}

#[test]
fn linux_follows_the_session_type() {
    assert_eq!(
        select(Os::Linux, env(&[("XDG_SESSION_TYPE", "wayland")])),
        Backend::LinuxWayland
    );
    assert_eq!(
        select(Os::Linux, env(&[("XDG_SESSION_TYPE", "x11")])),
        Backend::LinuxX11
    );
}

#[test]
fn session_type_wins_over_wayland_display() {
    assert_eq!(
        linux_backend(Some("x11"), Some("wayland-0")),
        Backend::LinuxX11
    );
}

#[test]
fn linux_falls_back_to_wayland_display() {
    assert_eq!(
        select(Os::Linux, env(&[("WAYLAND_DISPLAY", "wayland-0")])),
        Backend::LinuxWayland
    );
    assert_eq!(
        linux_backend(Some("tty"), Some("wayland-0")),
        Backend::LinuxWayland
    );
}

#[test]
fn linux_with_no_wayland_hint_is_x11() {
    assert_eq!(select(Os::Linux, env(&[])), Backend::LinuxX11);
    assert_eq!(linux_backend(None, Some("")), Backend::LinuxX11);
}

#[test]
fn this_build_selects_its_own_backend() {
    let expected = match current_os() {
        Os::MacOs => Backend::MacOs,
        Os::Windows => Backend::Windows,
        Os::Linux => current(),
    };
    assert_eq!(current(), expected);
    #[cfg(target_os = "macos")]
    assert_eq!(current_os(), Os::MacOs);
    #[cfg(target_os = "windows")]
    assert_eq!(current_os(), Os::Windows);
    #[cfg(target_os = "linux")]
    assert_eq!(current_os(), Os::Linux);
}

#[cfg(target_os = "macos")]
#[test]
fn macos_creates_the_macos_platform() {
    assert_eq!(create().info().os, Os::MacOs);
}

#[cfg(target_os = "linux")]
#[test]
fn linux_creates_the_linux_platform() {
    assert_eq!(create().info().os, Os::Linux);
}

#[cfg(target_os = "windows")]
#[test]
fn windows_creates_the_windows_platform() {
    assert_eq!(create().info().os, Os::Windows);
}

#[test]
fn the_process_environment_is_read_by_name() {
    assert!(process_env("PATH").is_some());
    assert_eq!(process_env("MOUSER_SURELY_NOT_SET_1234"), None);
}
