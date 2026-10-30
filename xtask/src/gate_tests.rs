use super::*;

fn excluded(path: &str) -> bool {
    regex_lite_match(EXCLUDE, path)
}

// EXCLUDE is a plain alternation of literals/escapes; match each alternative as a substring.
fn regex_lite_match(pattern: &str, path: &str) -> bool {
    pattern
        .trim_matches(|c| c == '(' || c == ')')
        .split('|')
        .any(|alt| path.contains(&alt.replace("\\.", ".")))
}

#[test]
fn exclude_matches_winapi_sys_with_both_separators() {
    assert!(excluded("apps/mouser-service-win/src/winapi_sys.rs"));
    assert!(excluded("apps\\mouser-service-win\\src\\winapi_sys.rs"));
}

#[test]
fn exclude_matches_no_other_service_win_source() {
    for f in ["winapi.rs", "session.rs", "main.rs"] {
        assert!(
            !excluded(&format!("apps/mouser-service-win/src/{f}")),
            "{f}"
        );
    }
}

const HELPER: &str = "XTASK_TEE_HELPER";

/// Not a test of its own: a portable child process for the `tee` tests, run
/// as this test binary with `XTASK_TEE_HELPER` set to the behaviour wanted.
#[test]
fn tee_helper() {
    let Ok(mode) = std::env::var(HELPER) else {
        return;
    };
    let code = match mode.as_str() {
        "wait" => {
            println!("first");
            std::io::stdout().flush().unwrap();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut line = String::new();
                std::io::stdin().read_line(&mut line).unwrap();
                tx.send(()).unwrap();
            });
            // The 30 s only bounds the failing case; a passing run never waits.
            match rx.recv_timeout(std::time::Duration::from_secs(30)) {
                Ok(()) => println!("released"),
                Err(_) => println!("timed-out"),
            }
            0
        }
        "both" => {
            println!("out");
            eprintln!("err");
            3
        }
        other => panic!("unknown {HELPER} mode {other}"),
    };
    std::io::stdout().flush().unwrap();
    std::process::exit(code);
}

fn helper(mode: &str) -> std::process::Child {
    use std::process::Stdio;
    Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "gate::tests::tee_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(HELPER, mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

/// A sink that, on seeing `first`, tells the child to carry on: the child
/// only prints `released` if `first` reached the sink while it was running.
/// (The test harness prints its own header before the helper's lines.)
struct Releasing {
    stdin: Option<std::process::ChildStdin>,
    seen: Vec<u8>,
}

impl Write for Releasing {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.seen.extend_from_slice(buf);
        if self.seen.ends_with(b"first\n")
            && let Some(mut stdin) = self.stdin.take()
        {
            stdin.write_all(b"go\n").unwrap();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn output_is_forwarded_while_the_child_runs() {
    let mut child = helper("wait");
    let mut out = Releasing {
        stdin: child.stdin.take(),
        seen: Vec::new(),
    };
    let (code, stdout, _) = tee(child, &mut out, &mut Vec::new());
    assert_eq!(code, 0);
    let seen = String::from_utf8(out.seen).unwrap();
    assert!(seen.ends_with("first\nreleased\n"), "{seen:?}");
    assert_eq!(stdout, seen);
}

#[test]
fn both_streams_are_forwarded_and_kept_with_the_exit_code() {
    let child = helper("both");
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (code, stdout, stderr) = tee(child, &mut out, &mut err);
    assert_eq!(code, 3);
    assert!(stdout.ends_with("out\n"), "{stdout:?}");
    assert!(stderr.ends_with("err\n"), "{stderr:?}");
    assert_eq!(out, stdout.as_bytes());
    assert_eq!(err, stderr.as_bytes());
}
