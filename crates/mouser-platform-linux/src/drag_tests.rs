use crate::{LinuxPlatform, Settings};
use mouser_platform::{Capability, Platform};
use std::collections::HashMap;

fn platform(session: &str) -> (LinuxPlatform, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("os-release"),
        "NAME=\"Testix\"\nVERSION_ID=\"3\"\n",
    )
    .unwrap();
    let settings = Settings {
        env: HashMap::from([("XDG_SESSION_TYPE".to_string(), session.to_string())]),
        sys_class_net: "/sys/class/net".into(),
        os_release: dir.path().join("os-release"),
        exe: "/opt/mouser/mouser".into(),
        device_wait: std::time::Duration::from_millis(300),
    };
    (LinuxPlatform::with_settings(settings), dir)
}

#[test]
fn there_is_no_file_drag_and_the_limitation_points_to_copy_and_paste() {
    for session in ["x11", "wayland"] {
        let (p, _dir) = platform(session);
        assert!(p.file_drag().is_none(), "{session}");
        let texts: Vec<_> = p
            .info()
            .limitations
            .into_iter()
            .filter(|l| l.capability == Capability::FileDrag)
            .map(|l| l.text)
            .collect();
        assert_eq!(
            texts,
            ["Dragging files between computers isn't available on Linux; use copy and paste"]
        );
    }
}
