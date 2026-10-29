//! Which control endpoint the `mouser` binary connects to (contracts/cli.md,
//! control-api.md): the service's pipe `\\.\pipe\mouser-service` when it
//! exists (Windows), otherwise this user's socket or pipe. A stand-in server
//! listens at the endpoint; the test passes when the CLI connects to it.
//! This is its own test binary because the service pipe is machine-wide: it
//! would otherwise catch the other tests' CLI processes.

use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

/// A stand-in server at one endpoint.
#[cfg(unix)]
struct Endpoint {
    listener: tokio::net::UnixListener,
}

#[cfg(unix)]
impl Endpoint {
    fn at(path: &std::path::Path) -> Self {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        Self {
            listener: tokio::net::UnixListener::bind(path).unwrap(),
        }
    }

    async fn accepted(&self) {
        self.listener.accept().await.unwrap();
    }
}

#[cfg(windows)]
struct Endpoint {
    pipe: tokio::net::windows::named_pipe::NamedPipeServer,
}

#[cfg(windows)]
impl Endpoint {
    fn at(path: &std::path::Path) -> Self {
        Self {
            pipe: tokio::net::windows::named_pipe::ServerOptions::new()
                .first_pipe_instance(true)
                .create(path)
                .unwrap(),
        }
    }

    async fn accepted(&self) {
        self.pipe.connect().await.unwrap();
    }
}

/// Run `mouser status` with `env` as its whole environment and assert that it
/// connects to `endpoint` before it exits.
async fn assert_connects(endpoint: Endpoint, env: &[(&str, &str)]) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mouser"))
        .arg("status")
        .env_clear()
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    tokio::select! {
        () = endpoint.accepted() => {}
        status = child.wait() => panic!("the CLI exited ({status:?}) without connecting"),
    }
    // The stand-in hangs up, as an engine that goes away does, and the CLI
    // exits by itself. Killed instead, it would leave the coverage data it
    // was writing broken, and one broken file fails the whole coverage merge.
    drop(endpoint);
    tokio::time::timeout(Duration::from_secs(20), child.wait())
        .await
        .expect("the CLI exits once the endpoint hangs up")
        .unwrap();
}

#[cfg(target_os = "macos")]
fn user_endpoint(home: &std::path::Path) -> (Endpoint, Vec<(&'static str, String)>) {
    let path = home.join("Library/Application Support/Mouser/mouser.sock");
    (
        Endpoint::at(&path),
        vec![("HOME", home.to_str().unwrap().to_string())],
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn user_endpoint(home: &std::path::Path) -> (Endpoint, Vec<(&'static str, String)>) {
    (
        Endpoint::at(&home.join("mouser.sock")),
        vec![("XDG_RUNTIME_DIR", home.to_str().unwrap().to_string())],
    )
}

#[cfg(windows)]
fn user_endpoint(_home: &std::path::Path) -> (Endpoint, Vec<(&'static str, String)>) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static USERS: AtomicU32 = AtomicU32::new(0);
    let user = format!(
        "endpoint{}n{}",
        std::process::id(),
        USERS.fetch_add(1, Ordering::Relaxed)
    );
    (
        Endpoint::at(std::path::Path::new(&format!(r"\\.\pipe\mouser-{user}"))),
        vec![("USERNAME", user)],
    )
}

/// The tests take turns: while one has the service's pipe (machine-wide)
/// open, the other's CLI would connect to it.
static ONE_AT_A_TIME: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn short_tempdir() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("mc");
    builder
        .tempdir_in("/tmp")
        .unwrap_or_else(|_| tempfile::tempdir().unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_service_the_cli_connects_to_the_users_endpoint() {
    let _turn = ONE_AT_A_TIME.lock().await;
    let home = short_tempdir();
    let (endpoint, env) = user_endpoint(home.path());
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    assert_connects(endpoint, &env).await;
}

#[cfg(windows)]
#[tokio::test(flavor = "multi_thread")]
async fn with_the_service_pipe_present_the_cli_connects_to_it_not_to_the_users_pipe() {
    let _turn = ONE_AT_A_TIME.lock().await;
    // The user's pipe exists too, and must not be the one chosen.
    let home = short_tempdir();
    let (_users, env) = user_endpoint(home.path());
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let service = Endpoint::at(std::path::Path::new(mouser_core::control::SERVICE_PIPE));
    assert_connects(service, &env).await;
}
