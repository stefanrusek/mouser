use clap::Args as ClapArgs;
use serde_json::json;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::process::Command;

const UI_DIR: &str = "apps/mouser-app/ui";
const EXCLUDE: &str = "(mouser-testkit|xtask|spikes|probes|/gen/|e2e)";

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

fn record(lane: &str, cmd: &mut Command, label: &str) -> bool {
    let out = cmd.output().expect("failed to spawn check");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    print!("{stdout}");
    eprint!("{stderr}");
    let rec = json!({
        "check": label,
        "command": format!("{cmd:?}"),
        "exit_code": code,
        "stdout": stdout,
        "stderr": stderr,
        "timestamp_utc": chrono::Utc::now().to_rfc3339(),
    });
    let dir = Path::new("specs/001-mouser-v1/evidence");
    fs::create_dir_all(dir).expect("evidence dir");
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
