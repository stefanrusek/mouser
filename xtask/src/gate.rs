use clap::Args as ClapArgs;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};

use crate::feature_dir;

const UI_DIR: &str = "apps/mouser-app/ui";
// OD-5: SYSTEM-only WinApi pass-through, verified by probe P2 and Story 7 R8/R9 (debt)
const EXCLUDE: &str = "(mouser-testkit|xtask|spikes|probes|/gen/|e2e|winapi_sys\\.rs)";

#[derive(ClapArgs)]
pub struct Args {
    /// Packages to gate.
    #[arg(long, num_args = 1.., value_delimiter = ',', conflicts_with_all = ["all", "e2e"])]
    packages: Vec<String>,
    /// Gate every workspace package.
    #[arg(long)]
    all: bool,
    /// Run an end-to-end journey (e.g. j1).
    #[arg(long)]
    e2e: Option<String>,
    /// Evidence lane name.
    #[arg(long, default_value = "default")]
    lane: String,
}

fn workspace_packages() -> Vec<String> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .expect("cargo metadata");
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).expect("metadata json");
    meta["packages"]
        .as_array()
        .expect("packages")
        .iter()
        .map(|p| p["name"].as_str().expect("name").to_string())
        .collect()
}

/// Copies `from` to `to` as it arrives and returns everything copied.
fn forward(mut from: impl Read, to: &mut impl Write) -> String {
    let (mut all, mut buf) = (Vec::new(), [0u8; 8192]);
    loop {
        let n = from.read(&mut buf).expect("read child output");
        if n == 0 {
            return String::from_utf8_lossy(&all).into_owned();
        }
        to.write_all(&buf[..n]).expect("forward child output");
        to.flush().expect("forward child output");
        all.extend_from_slice(&buf[..n]);
    }
}

/// Forwards the child's stdout and stderr live (so a hung check shows how far
/// it got) and returns its exit code with both streams for the evidence.
fn tee(
    mut child: Child,
    out: &mut (impl Write + Send),
    err: &mut (impl Write + Send),
) -> (i32, String, String) {
    let (stdout, stderr) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let (stdout, stderr) = std::thread::scope(|s| {
        let stderr = s.spawn(|| forward(stderr, err));
        (forward(stdout, out), stderr.join().unwrap())
    });
    let code = child.wait().expect("wait for check").code().unwrap_or(-1);
    (code, stdout, stderr)
}

fn record(lane: &str, cmd: &mut Command, label: &str) -> bool {
    let child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn check");
    let (code, stdout, stderr) = tee(child, &mut std::io::stdout(), &mut std::io::stderr());
    let rec = json!({
        "check": label,
        "command": format!("{cmd:?}"),
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
        "timestamp_utc": chrono::Utc::now().to_rfc3339(),
    });
    let dir = feature_dir::evidence_dir();
    fs::create_dir_all(&dir).expect("evidence dir");
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{lane}.jsonl")))
        .expect("evidence file");
    writeln!(f, "{rec}").expect("write evidence");
    code == 0
}

fn cargo(args: &[&str]) -> Command {
    let mut c = Command::new("cargo");
    c.args(args);
    c
}

pub fn run(args: Args) -> i32 {
    let lane = args.lane.as_str();
    let mut ok = true;
    if let Some(j) = &args.e2e {
        let test = format!("e2e_{j}");
        ok &= record(
            lane,
            &mut cargo(&[
                "test",
                "-p",
                "mouser-cli",
                "--features",
                "e2e",
                "--test",
                &test,
            ]),
            "e2e",
        );
        return i32::from(!ok);
    }
    let pkgs = if args.all {
        workspace_packages()
    } else {
        args.packages
    };
    let mut pargs: Vec<&str> = Vec::new();
    for p in &pkgs {
        pargs.extend(["-p", p]);
    }
    ok &= record(lane, cargo(&["fmt"]).args(&pargs).arg("--check"), "fmt");
    // --no-deps: without it clippy also lints workspace path dependencies,
    // so one lane's in-progress code would fail another lane's gate.
    ok &= record(
        lane,
        cargo(&["clippy"]).args(&pargs).args([
            "--all-targets",
            "--no-deps",
            "--",
            "-D",
            "warnings",
        ]),
        "clippy",
    );
    if pkgs.iter().any(|p| p == "mouser-app") {
        // The live Tauri shell only compiles with `gui`; lint it too.
        ok &= record(
            lane,
            &mut cargo(&[
                "clippy",
                "-p",
                "mouser-app",
                "--features",
                "gui",
                "--all-targets",
                "--no-deps",
                "--",
                "-D",
                "warnings",
            ]),
            "clippy-gui",
        );
    }
    ok &= record(lane, cargo(&["test"]).args(&pargs), "test");
    // Per-lane coverage target dir: llvm-cov cleans profraw files in its target
    // dir, so lanes gating concurrently must not share one.
    ok &= record(
        lane,
        cargo(&["llvm-cov"])
            .env(
                "CARGO_LLVM_COV_TARGET_DIR",
                format!("target/llvm-cov-{lane}"),
            )
            .args(&pargs)
            .args([
                "--show-missing-lines",
                "--fail-under-lines",
                "100",
                "--ignore-filename-regex",
                EXCLUDE,
            ]),
        "coverage",
    );
    if pkgs.iter().any(|p| p == "mouser-app") {
        ok &= record(
            lane,
            Command::new("node")
                .args([
                    "--test",
                    "--experimental-test-coverage",
                    "--test-coverage-lines=100",
                ])
                .current_dir(UI_DIR),
            "ui-coverage",
        );
    }
    i32::from(!ok)
}

#[cfg(test)]
#[path = "gate_tests.rs"]
mod tests;
