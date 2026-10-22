//! Shared test harness: in-process engines over `VirtualPlatform` + `SimNet`
//! serving their control socket in a temp dir, and the real `mouser` binary
//! pointed at that socket the way a user's shell would reach it: through the
//! per-OS default path (contracts/control-api.md), which the child derives from
//! its environment (`HOME`, `XDG_RUNTIME_DIR` or `USERNAME`).

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mouser_core::discovery::{Advert, Discovery, Found};
use mouser_core::{ControlClient, Engine, EngineParams};
use mouser_proto::config::Config;
use mouser_proto::control::{Method, Status};
use mouser_proto::ids::ComputerId;
use mouser_testkit::{SimNet, SimNode, VirtualPlatform};
use serde_json::{Value, json};
use tokio::process::{Child, Command};

pub mod release;

const WAIT: Duration = Duration::from_secs(20);

/// In-process stand-in for the network's mDNS (as in mouser-core's harness).
#[derive(Default)]
struct Registry {
    adverts: Mutex<BTreeMap<ComputerId, (Advert, SimNode)>>,
}

struct SimDiscovery {
    registry: Arc<Registry>,
    me: SimNode,
    own: Mutex<Option<ComputerId>>,
}

impl Discovery for SimDiscovery {
    fn announce(&self, advert: &Advert) {
        *self.own.lock().unwrap() = Some(advert.computer);
        self.registry
            .adverts
            .lock()
            .unwrap()
            .insert(advert.computer, (advert.clone(), self.me.clone()));
    }

    fn found(&self) -> Vec<Found> {
        let own = *self.own.lock().unwrap();
        let adverts = self.registry.adverts.lock().unwrap();
        adverts
            .values()
            .filter(|(ad, _)| Some(ad.computer) != own)
            .map(|(ad, node)| Found {
                computer: ad.computer,
                display_name: ad.display_name.clone(),
                protocol: ad.protocol,
                desk: ad.desk.clone(),
                addrs: vec![self.me.route_to(node)],
            })
            .collect()
    }

    fn stop(&self) {
        if let Some(own) = self.own.lock().unwrap().take() {
            self.registry.adverts.lock().unwrap().remove(&own);
        }
    }
}

/// What a finished `mouser` run printed.
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// Exit 0, and stdout as JSON.
    pub fn json(&self) -> Value {
        assert_eq!(self.code, 0, "stderr: {}", self.stderr);
        serde_json::from_str(&self.stdout).expect("stdout is one JSON document")
    }
}

/// One computer: a user's login environment and, when running, its engine.
pub struct Node {
    pub name: String,
    pub sim: SimNode,
    env: Vec<(&'static str, String)>,
    control_path: PathBuf,
    /// Where this user's `mouser` looks for `config.toml` and `logs/`.
    pub user_config_dir: PathBuf,
    id: Option<ComputerId>,
    _home: tempfile::TempDir,
    _config: tempfile::TempDir,
}

/// A port for one engine, in 20000-40000 (clear of the OS ephemeral range that
/// SimNet's relay sockets draw from). Every call in a process gets a distinct
/// one; it is deliberately not probed, because on macOS a just-closed probe
/// socket can make the engine's own bind fail.
fn free_port() -> u16 {
    static CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    20_000 + (std::process::id().wrapping_mul(97).wrapping_add(n) % 20_000) as u16
}

/// A user's home with the environment that makes the default control path
/// point inside it.
#[cfg(target_os = "macos")]
fn login(home: &std::path::Path) -> (Vec<(&'static str, String)>, PathBuf, PathBuf) {
    let dir = home.join("Library/Application Support/Mouser");
    std::fs::create_dir_all(&dir).unwrap();
    (
        vec![("HOME", home.to_str().unwrap().to_string())],
        dir.join("mouser.sock"),
        dir,
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
fn login(home: &std::path::Path) -> (Vec<(&'static str, String)>, PathBuf, PathBuf) {
    (
        vec![
            ("XDG_RUNTIME_DIR", home.to_str().unwrap().to_string()),
            (
                "XDG_CONFIG_HOME",
                home.join("config").to_str().unwrap().to_string(),
            ),
        ],
        home.join("mouser.sock"),
        home.join("config/mouser"),
    )
}

#[cfg(windows)]
fn login(home: &std::path::Path) -> (Vec<(&'static str, String)>, PathBuf, PathBuf) {
    use std::sync::atomic::{AtomicU64, Ordering};
    static PIPES: AtomicU64 = AtomicU64::new(0);
    let user = format!(
        "cli{}n{}",
        std::process::id(),
        PIPES.fetch_add(1, Ordering::Relaxed)
    );
    let pipe = PathBuf::from(format!(r"\\.\pipe\mouser-{user}"));
    (
        vec![
            ("USERNAME", user),
            ("APPDATA", home.to_str().unwrap().to_string()),
        ],
        pipe,
        home.join("Mouser"),
    )
}

/// A temp dir short enough for a Unix socket path.
fn short_tempdir() -> tempfile::TempDir {
    let mut builder = tempfile::Builder::new();
    builder.prefix("mc");
    if cfg!(unix) {
        builder.tempdir_in("/tmp").unwrap()
    } else {
        builder.tempdir().unwrap()
    }
}

/// Spawns hold the read side; a test that writes an executable (a copy of
/// `mouser`) holds the write side, so no child is forked while it is open.
pub static SPAWNS: std::sync::RwLock<()> = std::sync::RwLock::new(());

impl Node {
    pub fn id(&self) -> ComputerId {
        self.id.expect("node is running")
    }

    /// Where this computer's Mouser serves (or would serve) its control API.
    #[cfg(unix)]
    pub fn control_path(&self) -> &std::path::Path {
        &self.control_path
    }

    /// Run `mouser args...` as this computer's user and wait for it.
    pub async fn mouser(&self, args: &[&str]) -> Output {
        self.mouser_at(env!("CARGO_BIN_EXE_mouser").as_ref(), args)
            .await
    }

    /// Run the `mouser` at `exe` (a copy somewhere else) as this user.
    pub async fn mouser_at(&self, exe: &std::path::Path, args: &[&str]) -> Output {
        let child = self.spawn_at(exe, args);
        let out = child.wait_with_output().await.unwrap();
        Output {
            code: out.status.code().expect("mouser exits by itself"),
            stdout: String::from_utf8(out.stdout).unwrap(),
            stderr: String::from_utf8(out.stderr).unwrap(),
        }
    }

    /// Start `mouser args...` without waiting (for `desk join`).
    pub fn spawn(&self, args: &[&str]) -> Child {
        self.spawn_at(env!("CARGO_BIN_EXE_mouser").as_ref(), args)
    }

    fn spawn_at(&self, exe: &std::path::Path, args: &[&str]) -> Child {
        let mut command = Command::new(exe);
        command
            .args(args)
            .envs(self.env.iter().map(|(k, v)| (*k, v)))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Forking while another thread has an executable open for writing lets
        // the child inherit that descriptor, and exec then fails with
        // "Text file busy"; see `SPAWNS`.
        let _forking = SPAWNS.read().unwrap();
        command.spawn().unwrap()
    }

    /// The engine's `status`, asked directly over the control socket.
    pub async fn status(&self) -> Status {
        serde_json::from_value(self.call(Method::Status).await).unwrap()
    }

    /// Call a parameterless method directly over the control socket.
    pub async fn call(&self, method: Method) -> Value {
        let mut client = ControlClient::connect(&self.control_path).await.unwrap();
        client.call(method, Some(json!({}))).await.unwrap()
    }

    /// Poll `status` until `pred` holds.
    pub async fn wait_status(&self, what: &str, pred: impl Fn(&Status) -> bool) -> Status {
        let end = Instant::now() + WAIT;
        loop {
            let status = self.status().await;
            if pred(&status) {
                return status;
            }
            assert!(
                Instant::now() < end,
                "{}: timed out waiting for {what}\n{status:#?}",
                self.name
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    /// Poll `mouser requests --json` until a join request is pending; its id.
    pub async fn pending_request(&self) -> u64 {
        let end = Instant::now() + WAIT;
        loop {
            let requests = self.mouser(&["--json", "requests"]).await.json();
            if let Some(first) = requests.as_array().unwrap().first() {
                return first["request_id"].as_u64().unwrap();
            }
            assert!(Instant::now() < end, "no join request arrived");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

/// One simulated network and its computers.
pub struct Lab {
    net: SimNet,
    registry: Arc<Registry>,
    engines: Mutex<Vec<Engine>>,
}

impl Lab {
    pub fn new() -> Self {
        Self {
            net: SimNet::new(),
            registry: Arc::new(Registry::default()),
            engines: Mutex::new(Vec::new()),
        }
    }

    /// Stop every engine and wait for its tasks. Each test ends with this,
    /// before its nodes (and their temp dirs) are dropped.
    pub async fn shutdown(self) {
        let engines = std::mem::take(&mut *self.engines.lock().unwrap());
        for engine in engines {
            engine.shutdown().await;
        }
    }

    /// A computer whose engine is running and serving its control socket.
    pub async fn add(&self, name: &str) -> Node {
        self.add_with(name, VirtualPlatform::new()).await
    }

    /// Like `add`, on a platform the test has already set up.
    pub async fn add_with(&self, name: &str, platform: VirtualPlatform) -> Node {
        self.add_tuned(name, platform, |_| {}, |_| {}).await
    }

    /// Like `add`, with the config and engine parameters adjusted first.
    pub async fn add_tuned(
        &self,
        name: &str,
        platform: VirtualPlatform,
        tune_config: impl FnOnce(&mut Config),
        tune_params: impl FnOnce(&mut EngineParams),
    ) -> Node {
        let mut node = self.offline(name);
        let port = free_port();
        let mut config = Config::default();
        config.advanced.port = u32::from(port);
        tune_config(&mut config);
        mouser_core::config::save(node._config.path(), &config);
        let sim = self
            .net
            .add_node(name, SocketAddr::from(([127, 0, 0, 1], port)));
        sim.bind_platform(&platform);
        let mut params = EngineParams::new(
            Arc::new(platform),
            node._config.path().to_path_buf(),
            node.control_path.clone(),
            name.to_string(),
            "0.1.0".to_string(),
        );
        params.discovery = Some(Arc::new(SimDiscovery {
            registry: self.registry.clone(),
            me: sim.clone(),
            own: Mutex::new(None),
        }));
        tune_params(&mut params);
        node.sim = sim;
        let engine = Engine::start(params).await;
        node.id = Some(engine.computer_id());
        self.engines.lock().unwrap().push(engine);
        node
    }

    /// A user whose Mouser is not running.
    pub fn offline(&self, name: &str) -> Node {
        let home = short_tempdir();
        let (env, control_path, user_config_dir) = login(home.path());
        Node {
            name: name.to_string(),
            sim: self.net.add_node(
                &format!("{name}-offline"),
                SocketAddr::from(([127, 0, 0, 1], 9)),
            ),
            env,
            control_path,
            user_config_dir,
            id: None,
            _home: home,
            _config: tempfile::tempdir().unwrap(),
        }
    }
}
