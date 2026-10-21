//! Pre-login agent (Story 12): a LaunchAgent limited to the login window that
//! starts Mouser headless, so the shared keyboard works at the login screen.
//!
//! Installing only writes the plist; launchd loads `LoginWindow` agents by
//! itself. The system folder needs administrator rights, which the installer
//! provides, so the folder is a parameter.

use std::path::PathBuf;

/// launchd label, also the plist file stem.
pub const LABEL: &str = "com.mouser.login-window";

/// Folder launchd scans for agents that apply to every user and the login window.
const SYSTEM_DIR: &str = "/Library/LaunchAgents";

/// The pre-login agent for one executable.
pub struct LoginAgent {
    dir: PathBuf,
    exe: PathBuf,
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub(crate) fn plist(exe: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
		<string>--headless</string>
	</array>
	<key>LimitLoadToSessionType</key>
	<string>LoginWindow</string>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
</dict>
</plist>
"#,
        xml_escape(exe)
    )
}

impl LoginAgent {
    /// An agent whose plist goes into `dir`.
    pub fn new(dir: impl Into<PathBuf>, exe: impl Into<PathBuf>) -> Self {
        LoginAgent {
            dir: dir.into(),
            exe: exe.into(),
        }
    }

    /// The agent installed machine-wide in `/Library/LaunchAgents`.
    pub fn system(exe: impl Into<PathBuf>) -> Self {
        Self::new(SYSTEM_DIR, exe)
    }

    /// Where the plist is written.
    pub fn path(&self) -> PathBuf {
        self.dir.join(format!("{LABEL}.plist"))
    }

    pub fn installed(&self) -> bool {
        self.path().exists()
    }

    /// Writes (or rewrites) the plist.
    pub fn install(&self) {
        let exe = self.exe.to_str().expect("executable path is UTF-8");
        std::fs::write(self.path(), plist(exe)).expect("write login agent plist");
    }

    /// Removes the plist; removing an agent that is not installed is a no-op.
    pub fn uninstall(&self) {
        if self.installed() {
            std::fs::remove_file(self.path()).expect("remove login agent plist");
        }
    }
}
