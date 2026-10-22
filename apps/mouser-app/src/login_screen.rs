//! "Allow control at the login screen" (macOS, Story 12).
//!
//! The headless Mouser that the login window starts cannot reach the user's
//! keychain, so the user's session first stages what it needs (the device
//! and desk keys, the desk membership, the settings) in a private folder, and
//! then one administrator prompt runs this same executable as root with
//! [`FLAG`]. As root it moves the staged files into the machine directory and
//! writes the login-window agent. Declining the prompt changes nothing.
//!
//! The privileged step goes through [`Admin`], so tests never escalate.

use std::path::{Path, PathBuf};

use mouser_core::engine_headless::FileSecrets;
use mouser_platform_macos::login_agent::LoginAgent;

use crate::headless::secrets_dir;
use crate::host::Host;

/// The argument that makes this executable do the root-side install; its
/// value is the staging folder.
pub const FLAG: &str = "--install-login-screen";

/// The staging folder named by [`FLAG`], if the command line asks for the
/// root-side install.
pub fn install_arg(args: &[String]) -> Option<PathBuf> {
    let at = args.iter().position(|a| a == FLAG)?;
    args.get(at + 1).map(PathBuf::from)
}

/// The owner declined the administrator prompt.
#[derive(Debug, PartialEq, Eq)]
pub struct Refused;

/// Runs a program as administrator after one OS prompt.
pub trait Admin: Send + Sync {
    fn run(&self, program: &Path, args: &[String]) -> Result<(), Refused>;
}

/// Stage the machine-scope files in `staging`, run this executable (`exe`)
/// as administrator to install them, and remove the staged files again.
/// `user_config_dir` is where the user's own `config.toml` lives.
pub fn allow(
    host: &Host,
    user_config_dir: &Path,
    staging: &Path,
    exe: &Path,
    admin: &dyn Admin,
) -> Result<(), String> {
    let _ = std::fs::remove_dir_all(staging);
    std::fs::create_dir_all(staging).expect("staging folder can be created");
    set_mode(staging, 0o700);
    let config = user_config_dir.join(mouser_core::config::FILE);
    if config.exists() {
        std::fs::copy(&config, staging.join(mouser_core::config::FILE))
            .expect("config can be staged");
    }
    host.export_machine_scope(staging, &mut FileSecrets::new(secrets_dir(staging)));
    let result = admin.run(
        exe,
        &[
            FLAG.to_string(),
            staging.to_str().expect("UTF-8 path").to_string(),
        ],
    );
    std::fs::remove_dir_all(staging).expect("staging folder can be removed");
    result.map_err(|Refused| {
        "Mouser was not given permission, so nothing was changed. Choose \"Allow control at the login screen\" again when you are ready to enter your password."
            .to_string()
    })
}

/// The root-side install: replace the machine directory's contents with the
/// staged files, readable by root only, and write the login-window agent.
pub fn install_as_root(staging: &Path, dest: &Path, agent: &LoginAgent) {
    let _ = std::fs::remove_dir_all(secrets_dir(dest));
    copy_private(staging, dest);
    agent.install();
}

fn copy_private(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("machine folder can be created");
    set_mode(to, 0o700);
    for entry in std::fs::read_dir(from).expect("staging folder is readable") {
        let entry = entry.expect("staged entry is readable");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("staged entry has a type").is_dir() {
            copy_private(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("staged file can be copied");
        }
    }
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .expect("permissions can be set");
}

/// `program` and `args` as one shell command line, every word quoted.
pub fn shell_line(program: &Path, args: &[String]) -> String {
    let quote = |word: &str| format!("'{}'", word.replace('\'', "'\\''"));
    std::iter::once(program.to_str().expect("UTF-8 path"))
        .chain(args.iter().map(String::as_str))
        .map(quote)
        .collect::<Vec<_>>()
        .join(" ")
}

/// What `osascript` did.
#[derive(Clone)]
pub struct Outcome {
    pub success: bool,
    pub stderr: String,
}

/// [`Admin`] through the macOS administrator prompt. The closure runs
/// `osascript` with the given arguments (the live app spawns the process).
pub struct Osascript<F>(pub F);

impl<F: Fn(&[String]) -> Outcome + Send + Sync> Admin for Osascript<F> {
    fn run(&self, program: &Path, args: &[String]) -> Result<(), Refused> {
        let line = shell_line(program, args)
            .replace('\\', "\\\\")
            .replace('"', "\\\"");
        let script = format!("do shell script \"{line}\" with administrator privileges");
        let outcome = (self.0)(&["-e".to_string(), script]);
        if outcome.success {
            Ok(())
        } else if outcome.stderr.contains("User canceled") {
            Err(Refused)
        } else {
            panic!("the administrator step failed: {}", outcome.stderr);
        }
    }
}

/// The Tauri command behind the settings button.
#[tauri::command]
pub fn allow_login_screen(
    host: tauri::State<'_, Host>,
    admin: tauri::State<'_, Box<dyn Admin>>,
) -> Result<(), String> {
    allow(
        &host,
        &mouser_core::config::default_dir(),
        &std::env::temp_dir().join(format!("mouser-login-screen-{}", std::process::id())),
        &std::env::current_exe().expect("the executable has a path"),
        admin.as_ref(),
    )
}
