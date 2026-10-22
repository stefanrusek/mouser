//! A private Xvfb for the tests.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

pub(crate) struct Xvfb {
    child: Child,
    pub(crate) display: String,
}

impl Xvfb {
    pub(crate) fn start() -> Self {
        let mut child = Command::new("Xvfb")
            .args([
                "-displayfd",
                "1",
                "-screen",
                "0",
                "1920x1080x24",
                "-nolisten",
                "tcp",
                "-noreset",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("Xvfb must be installed to run the X11 tests");
        let mut number = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut number)
            .unwrap();
        Xvfb {
            child,
            display: format!(":{}", number.trim()),
        }
    }

    /// The environment of a session on this server.
    pub(crate) fn env(&self) -> HashMap<String, String> {
        HashMap::from([("DISPLAY".to_string(), self.display.clone())])
    }
}

impl Drop for Xvfb {
    fn drop(&mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }
}
