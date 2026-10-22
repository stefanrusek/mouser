//! The world of journey J1: two real computers. Computer A is the controller,
//! a Mac, and the machine running the test. Computer B is a Windows 11 VM in
//! Parallels on that same Mac, reached over SSH at `$MOUSER_J1_PEER` (anything
//! `ssh` accepts, e.g. `me@windows-vm.local`; key-based login, no prompts).
//!
//! Everything sent to B is a PowerShell script passed as `-EncodedCommand`, so
//! it does not depend on the OpenSSH server's default shell or on quoting.
//! Settings (all environment variables):
//!
//! | Variable              | Meaning                                          |
//! |-----------------------|--------------------------------------------------|
//! | `MOUSER_J1_PEER`      | `user@host` of the Windows VM (required)         |
//! | `MOUSER_J1_KEY`       | ssh private key for it (optional)                |
//! | `MOUSER_J1_VM`        | Parallels VM name for `prlctl` (required to sleep) |
//! | `MOUSER_J1_LOCAL_CLI` | the `mouser` CLI on the Mac (default `mouser`)   |
//! | `MOUSER_J1_PEER_CLI`  | `mouser.exe` on the VM (default `%LOCALAPPDATA%\Mouser\mouser.exe`) |
//! | `MOUSER_J1_PEER_APP`  | the Mouser app exe on the VM (default `%LOCALAPPDATA%\Mouser\mouser-app.exe`) |

use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

pub const PEER_ENV: &str = "MOUSER_J1_PEER";
const WAIT: Duration = Duration::from_secs(60);

/// The folder on B that holds the helper scripts and the typed-into file.
pub const PUBLIC: &str = r"C:\Users\Public";

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

pub fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("set {name}; see tests/e2e/README.md"))
}

fn env_or(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.to_string())
}

/// Quote one word as a PowerShell single-quoted string.
pub fn ps_quote(word: &str) -> String {
    format!("'{}'", word.replace('\'', "''"))
}

/// Standard base64.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The `-EncodedCommand` argument for `script`: base64 of its UTF-16LE text.
fn encoded(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&utf16)
}

impl Computer {
    /// The ssh command that runs the PowerShell `script` on B.
    fn ssh(script: &str) -> Command {
        let mut command = Command::new("ssh");
        command.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5"]);
        if let Ok(key) = std::env::var("MOUSER_J1_KEY") {
            command.args(["-o", "IdentitiesOnly=yes", "-i", &key]);
        }
        command.arg(env(PEER_ENV));
        let script = format!("$ProgressPreference = 'SilentlyContinue'; {script}");
        command.arg(format!(
            "powershell -NoProfile -NonInteractive -EncodedCommand {}",
            encoded(&script)
        ));
        command
    }

    /// The command line that runs `program args...` on this computer. The
    /// program `mouser` is this computer's installed CLI.
    fn command(self, program: &str, args: &[&str]) -> Command {
        match self {
            Self::A => {
                let program = if program == "mouser" {
                    env_or("MOUSER_J1_LOCAL_CLI", "mouser")
                } else {
                    program.to_string()
                };
                let mut command = Command::new(program);
                command.args(args);
                command
            }
            Self::B => {
                let head = if program == "mouser" {
                    let cli = env_or("MOUSER_J1_PEER_CLI", r"$env:LOCALAPPDATA\Mouser\mouser.exe");
                    format!("& \"{cli}\"")
                } else {
                    format!("& {}", ps_quote(program))
                };
                let words: Vec<String> = args.iter().map(|a| ps_quote(a)).collect();
                Self::ssh(&format!("{head} {}; exit $LASTEXITCODE", words.join(" ")))
            }
        }
    }

    /// Run a shell script here (`sh` on the Mac, PowerShell on the VM).
    pub fn script(self, script: &str) -> Output {
        let mut command = match self {
            Self::A => {
                let mut command = Command::new("sh");
                command.args(["-c", script]);
                command
            }
            Self::B => Self::ssh(&format!("{script}; exit $LASTEXITCODE")),
        };
        command
            .stdin(Stdio::null())
            .output()
            .unwrap_or_else(|e| panic!("cannot run a script: {e}"))
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

    /// Whether this computer answers over ssh.
    pub fn reachable(self) -> bool {
        self.script("exit 0").status.success()
    }

    /// Run `body` (PowerShell) in the interactive desktop session of B's
    /// logged-in user. An ssh session is not in that session, so GUI programs
    /// started from it would have no desktop: a scheduled task with `/it`
    /// runs them where the user is.
    pub fn interactive(self, name: &str, body: &str) {
        assert!(matches!(self, Self::B), "only B has a separate session");
        let script = format!(
            "$p = '{PUBLIC}\\mouser-j1-{name}.ps1'; \
             Set-Content -LiteralPath $p -Value {body}; \
             schtasks /create /f /tn mouser-j1-{name} /sc once /st 00:00 /it \
               /tr \"powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File $p\"; \
             if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}; \
             schtasks /run /tn mouser-j1-{name}",
            body = ps_quote(body)
        );
        let out = self.script(&script);
        assert!(
            out.status.success(),
            "start {name} in B's desktop session: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The Mouser app exe on B.
    pub fn peer_app() -> String {
        env_or(
            "MOUSER_J1_PEER_APP",
            r"$env:LOCALAPPDATA\Mouser\mouser-app.exe",
        )
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
