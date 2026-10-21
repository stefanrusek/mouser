use crate::login_agent::{LABEL, LoginAgent, plist};
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("mouser-login-agent-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `plutil` only reads the file; it never touches launchd.
fn plutil(args: &[&str], file: &Path) -> String {
    let out = Command::new("/usr/bin/plutil")
        .args(args)
        .arg(file)
        .output()
        .unwrap();
    assert!(out.status.success(), "plutil {args:?}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn install_writes_a_login_window_only_headless_agent() {
    let dir = scratch("install");
    let agent = LoginAgent::new(&dir, "/Applications/Mouser.app/Contents/MacOS/mouser");
    assert!(!agent.installed());
    agent.install();
    assert!(agent.installed());
    let file = dir.join(format!("{LABEL}.plist"));
    plutil(&["-lint"], &file);
    assert_eq!(
        plutil(
            &["-extract", "LimitLoadToSessionType", "raw", "-o", "-"],
            &file
        ),
        "LoginWindow"
    );
    assert_eq!(
        plutil(&["-extract", "Label", "raw", "-o", "-"], &file),
        LABEL
    );
    assert_eq!(
        plutil(&["-extract", "ProgramArguments.0", "raw", "-o", "-"], &file),
        "/Applications/Mouser.app/Contents/MacOS/mouser"
    );
    assert_eq!(
        plutil(&["-extract", "ProgramArguments.1", "raw", "-o", "-"], &file),
        "--headless"
    );
    assert_eq!(
        plutil(&["-extract", "RunAtLoad", "raw", "-o", "-"], &file),
        "true"
    );
    assert_eq!(
        plutil(&["-extract", "KeepAlive", "raw", "-o", "-"], &file),
        "true"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn install_twice_rewrites_and_uninstall_removes() {
    let dir = scratch("uninstall");
    let agent = LoginAgent::new(&dir, "/opt/mouser");
    agent.install();
    agent.install();
    agent.uninstall();
    assert!(!agent.installed());
    assert!(!dir.join(format!("{LABEL}.plist")).exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_plist_escapes_xml_in_the_executable_path() {
    let text = plist("/Users/a&b/<m>");
    assert!(text.contains("<string>/Users/a&amp;b/&lt;m&gt;</string>"));
}

#[test]
fn the_system_agent_lives_in_the_library_launch_agents_folder() {
    let agent = LoginAgent::system("/opt/mouser");
    assert_eq!(
        agent.path(),
        Path::new("/Library/LaunchAgents/com.mouser.login-window.plist")
    );
}

#[test]
fn uninstalling_when_not_installed_is_a_no_op() {
    let dir = scratch("absent");
    LoginAgent::new(&dir, "/opt/mouser").uninstall();
    std::fs::remove_dir_all(dir).unwrap();
}
