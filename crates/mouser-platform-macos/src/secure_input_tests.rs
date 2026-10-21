use crate::secure_input::{Probe, SystemProbe, holder};
use mouser_platform::SecureInputHolder;

struct Fake {
    enabled: bool,
    pid: Option<u32>,
    app: Option<&'static str>,
    path: Option<&'static str>,
}

impl Probe for Fake {
    fn enabled(&self) -> bool {
        self.enabled
    }
    fn holder_pid(&self) -> Option<u32> {
        self.pid
    }
    fn app_name(&self, _pid: u32) -> Option<String> {
        self.app.map(String::from)
    }
    fn process_path(&self, _pid: u32) -> Option<String> {
        self.path.map(String::from)
    }
}

fn held() -> Fake {
    Fake {
        enabled: true,
        pid: Some(905),
        app: Some("Warp"),
        path: Some("/Applications/Warp.app/Contents/MacOS/stable"),
    }
}

#[test]
fn nothing_held_is_none() {
    let off = Fake {
        enabled: false,
        ..held()
    };
    assert_eq!(holder(&off), None);
}

#[test]
fn holder_is_named_by_nsrunningapplication() {
    assert_eq!(
        holder(&held()),
        Some(SecureInputHolder {
            pid: 905,
            app_name: "Warp".into()
        })
    );
}

#[test]
fn falls_back_to_the_process_path_file_name() {
    let probe = Fake {
        app: None,
        ..held()
    };
    assert_eq!(holder(&probe).unwrap().app_name, "stable");
}

#[test]
fn unnameable_holder_gets_the_generic_name() {
    let no_path = Fake {
        app: None,
        path: None,
        ..held()
    };
    let h = holder(&no_path).unwrap();
    assert_eq!((h.pid, h.app_name.as_str()), (905, "another application"));
    let root_path = Fake {
        app: None,
        path: Some("/"),
        ..held()
    };
    assert_eq!(holder(&root_path).unwrap().app_name, "another application");
}

#[test]
fn absent_pid_key_gives_a_generic_holder() {
    let probe = Fake {
        pid: None,
        ..held()
    };
    assert_eq!(
        holder(&probe),
        Some(SecureInputHolder {
            pid: 0,
            app_name: "another application".into()
        })
    );
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn EnableSecureEventInput() -> i32;
    fn DisableSecureEventInput() -> i32;
}

/// Turns Secure Input off again even if the test panics.
struct SecureInputOn;

impl SecureInputOn {
    fn new() -> Self {
        assert_eq!(unsafe { EnableSecureEventInput() }, 0);
        SecureInputOn
    }
}

impl Drop for SecureInputOn {
    fn drop(&mut self) {
        unsafe { DisableSecureEventInput() };
    }
}

#[test]
fn system_probe_sees_secure_input_taken_by_this_process() {
    let probe = SystemProbe;
    // Another app (a terminal with secure keyboard entry) may hold it already.
    let before = probe.enabled();
    let on = SecureInputOn::new();
    assert!(probe.enabled());
    let h = holder(&probe).unwrap();
    assert!(h.pid > 0);
    assert!(!h.app_name.is_empty());
    drop(on);
    assert_eq!(probe.enabled(), before);
}

#[test]
fn system_probe_names_processes() {
    let probe = SystemProbe;
    let me = std::process::id();
    let path = probe.process_path(me).unwrap();
    assert!(path.contains("mouser_platform_macos"));
    assert_eq!(probe.process_path(u32::MAX >> 1), None);
    assert_eq!(probe.app_name(u32::MAX >> 1), None);
    let dock = probe_dock_pid();
    assert_eq!(probe.app_name(dock).unwrap(), "Dock");
}

fn probe_dock_pid() -> u32 {
    let out = std::process::Command::new("/usr/bin/pgrep")
        .args(["-x", "Dock"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
