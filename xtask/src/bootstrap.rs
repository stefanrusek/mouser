use std::process::{Command, Stdio};

fn succeeds(cmd: &mut Command) -> bool {
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn must(cmd: &mut Command) {
    let status = cmd.status().expect("failed to spawn command");
    assert!(status.success(), "command failed: {cmd:?}");
}

pub fn run() {
    if !succeeds(Command::new("cargo").args(["llvm-cov", "--version"])) {
        must(Command::new("cargo").args([
            "install",
            "cargo-llvm-cov",
            "--version",
            "0.9.1",
            "--locked",
        ]));
    }
    if succeeds(Command::new("node").arg("--version")) {
        must(
            Command::new("npm")
                .args(["ci"])
                .current_dir("apps/mouser-app/ui"),
        );
    }
}
