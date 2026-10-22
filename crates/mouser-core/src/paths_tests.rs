use std::path::{Path, PathBuf};

use mouser_platform::Os;

use crate::paths::{Env, PORTABLE_DIR, Paths};

fn env(root: &Path) -> Env {
    Env {
        home: Some(root.join("home")),
        appdata: Some(root.join("appdata")),
        xdg_config_home: None,
    }
}

/// A drive holding the executable, with `MouserData/` beside it when asked.
fn drive(with_data: bool) -> (tempfile::TempDir, PathBuf) {
    let drive = tempfile::tempdir().unwrap();
    if with_data {
        std::fs::create_dir(drive.path().join(PORTABLE_DIR)).unwrap();
    }
    let exe = drive.path().join("mouser");
    (drive, exe)
}

#[test]
fn mouser_data_beside_the_executable_is_where_everything_lives() {
    let computer = tempfile::tempdir().unwrap();
    let (drive, exe) = drive(true);
    for os in [Os::MacOs, Os::Windows, Os::Linux] {
        let paths = Paths::resolve(&exe, os, &env(computer.path()));
        assert!(paths.portable);
        assert_eq!(paths.dir, drive.path().join("MouserData"));
    }
    // Nothing is written on the computer by resolving.
    assert_eq!(std::fs::read_dir(computer.path()).unwrap().count(), 0);
}

#[test]
fn without_mouser_data_the_per_os_paths_are_used() {
    let (_drive, exe) = drive(false);
    let root = Path::new("/r");
    let e = env(root);
    let at = |os| Paths::resolve(&exe, os, &e);
    assert!(!at(Os::MacOs).portable);
    assert_eq!(
        at(Os::MacOs).dir,
        root.join("home/Library/Application Support/Mouser")
    );
    assert_eq!(at(Os::Windows).dir, root.join("appdata/Mouser"));
    assert_eq!(at(Os::Linux).dir, root.join("home/.config/mouser"));
    let xdg = Env {
        xdg_config_home: Some(root.join("xdg")),
        ..e
    };
    assert_eq!(
        Paths::resolve(&exe, Os::Linux, &xdg).dir,
        root.join("xdg/mouser")
    );
}

#[test]
fn a_file_named_mouser_data_does_not_make_it_portable() {
    let (drive, exe) = drive(false);
    std::fs::write(drive.path().join(PORTABLE_DIR), b"").unwrap();
    assert!(!Paths::resolve(&exe, Os::Linux, &env(Path::new("/r"))).portable);
}

#[test]
fn portable_secrets_are_files_on_the_drive_and_installed_secrets_are_the_platforms() {
    let (drive, exe) = drive(true);
    let paths = Paths::resolve(&exe, Os::Linux, &env(Path::new("/r")));
    let mut store = paths.secrets().expect("portable keeps its own secrets");
    store.set("device_key", b"k").unwrap();
    assert_eq!(
        std::fs::read(drive.path().join("MouserData/secrets/device_key")).unwrap(),
        b"k"
    );
    let (_other, exe) = self::drive(false);
    assert!(
        Paths::resolve(&exe, Os::Linux, &env(Path::new("/r")))
            .secrets()
            .is_none()
    );
}

#[test]
fn the_environment_is_read_from_the_process() {
    let env = Env::from_process();
    assert_eq!(
        env.home,
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
    );
    assert_eq!(env.appdata, std::env::var_os("APPDATA").map(PathBuf::from));
    assert_eq!(
        env.xdg_config_home,
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
    );
}
