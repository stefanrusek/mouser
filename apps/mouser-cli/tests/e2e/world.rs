//! The world of journey J1: two real Macs. Computer A is the machine running
//! the test; computer B is reached over SSH at `$MOUSER_J1_PEER` (anything
//! `ssh` accepts, e.g. `me@mac-mini.local`; key-based login, no prompts).

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const PEER_ENV: &str = "MOUSER_J1_PEER";
const WAIT: Duration = Duration::from_secs(60);

#[derive(Debug, Default, cucumber::World)]
pub struct J1World {
    /// A `mouser desk join` on B that is still waiting for the decision.
    pub join: Option<Child>,
    /// What that join printed once it finished.
    pub joined: Option<Output>,
}

/// Which of the two computers a command runs on.
#[derive(Clone, Copy)]
pub enum Computer {
    A,
    B,
}

fn peer() -> String {
    std::env::var(PEER_ENV)
        .unwrap_or_else(|_| panic!("set {PEER_ENV} to the second Mac's ssh host"))
}

/// Quote one word for the remote POSIX shell.
fn quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', r"'\''"))
}

impl Computer {
    /// The command line that runs `program args...` on this computer.
    fn command(self, program: &str, args: &[&str]) -> Command {
        match self {
            Self::A => {
                let mut command = Command::new(program);
                command.args(args);
                command
            }
            Self::B => {
                let line = std::iter::once(program)
                    .chain(args.iter().copied())
                    .map(quote)
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut command = Command::new("ssh");
                command.args([
                    "-o",
                    "BatchMode=yes",
                    "-o",
                    "ConnectTimeout=5",
                    &peer(),
                    &line,
                ]);
                command
            }
        }
    }

    /// Run `program args...` here and wait for it.
    pub fn run(self, program: &str, args: &[&str]) -> Output {
        self.command(program, args)
            .stdin(Stdio::null())
            .output()
            .unwrap_or_else(|e| panic!("cannot run {program}: {e}"))
    }

    /// Start `program args...` here without waiting.
    pub fn start(self, program: &str, args: &[&str]) -> Child {
        self.command(program, args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {program}: {e}"))
    }

    /// `mouser args...` on this computer.
    pub fn mouser(self, args: &[&str]) -> Output {
        self.run("mouser", args)
    }

    /// `mouser --json args...` on this computer, which must succeed.
    pub fn mouser_json(self, args: &[&str]) -> serde_json::Value {
        let mut all = vec!["--json"];
        all.extend(args);
        let out = self.mouser(&all);
        assert!(
            out.status.success(),
            "mouser {all:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("mouser --json prints JSON")
    }

    /// Poll `check` until it returns `Some`, or fail after a minute.
    pub fn wait_for<T>(self, what: &str, mut check: impl FnMut(Self) -> Option<T>) -> T {
        let end = Instant::now() + WAIT;
        loop {
            if let Some(found) = check(self) {
                return found;
            }
            assert!(Instant::now() < end, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}
