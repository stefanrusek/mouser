//! Updates in the app (Story 10): the tray says which version runs and which
//! is available, "Update to Y" installs it, and the installer handed to the
//! engine applies a bundle the engine has already verified.
//!
//! `tauri-plugin-updater` installs only from an endpoint, and checks a
//! signature over the bundle. The engine has verified the bundle against the
//! Mouser key and holds no signature to pass on, so the installer vouches for
//! it: it signs the bundle with a key made for this one install, serves it to
//! the updater on a loopback endpoint, and gives the updater that key to
//! check against. The updater then does what it does for any update: checks,
//! replaces the app, and Mouser restarts.

use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer as _, SigningKey};
use mouser_core::ControlHandle;
use mouser_core::release::{Bundle, Installer};
use mouser_proto::control::{Method, RpcError, UpdateResult};
use serde_json::Value;

use crate::tray::TrayController;

/// How often the tray asks `update.check`. The engine looks for releases on
/// the internet by itself on its own schedule.
pub const POLL: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// The platform key the updater is told to look for in the manifest.
pub const TARGET: &str = "lan";

/// Installs from a loopback endpoint: the live app points `tauri-plugin-updater`
/// at it (with `target` as the platform key and `pubkey` as the trusted key),
/// downloads, installs and restarts.
pub trait Applier: Send + Sync {
    fn install(&self, endpoint: &str, target: &str, pubkey: &str) -> Result<(), String>;
}

/// The installer for `EngineParams.installer`.
pub fn installer(applier: Arc<dyn Applier>) -> Installer {
    Arc::new(move |bundle| {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("the system has random numbers");
        let key = Key::new(seed);
        let server = Server::start(&key, &bundle);
        let result = applier.install(&server.endpoint(), TARGET, &key.pubkey());
        server.stop();
        result
    })
}

/// A minisign key made for one install.
pub struct Key {
    key: SigningKey,
    id: [u8; 8],
}

impl Key {
    pub fn new(seed: [u8; 32]) -> Self {
        let mut id = [0u8; 8];
        id.copy_from_slice(&seed[..8]);
        Self {
            key: SigningKey::from_bytes(&seed),
            id,
        }
    }

    /// The base64 of the `.pub` file.
    pub fn pubkey(&self) -> String {
        let mut raw = b"Ed".to_vec();
        raw.extend(self.id);
        raw.extend(self.key.verifying_key().as_bytes());
        STANDARD.encode(format!(
            "untrusted comment: minisign public key\n{}\n",
            STANDARD.encode(raw)
        ))
    }

    /// The base64 of the `.sig` file for `data`.
    pub fn sign(&self, data: &[u8]) -> String {
        let signature = self.key.sign(data).to_bytes();
        let comment = "mouser lan update";
        let mut raw = b"Ed".to_vec();
        raw.extend(self.id);
        raw.extend(signature);
        let mut signed = signature.to_vec();
        signed.extend(comment.as_bytes());
        let global = self.key.sign(&signed).to_bytes();
        STANDARD.encode(format!(
            "untrusted comment: signature\n{}\ntrusted comment: {comment}\n{}\n",
            STANDARD.encode(raw),
            STANDARD.encode(global)
        ))
    }
}

/// The manifest the updater reads, for a bundle served at `bundle_url`.
fn manifest(key: &Key, bundle: &Bundle, bundle_url: &str) -> String {
    serde_json::json!({
        "version": bundle.version,
        "platforms": {
            TARGET: {"signature": key.sign(&bundle.bytes), "url": bundle_url}
        }
    })
    .to_string()
}

/// A loopback HTTP server for the manifest and the bundle.
pub struct Server {
    port: u16,
    stopping: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl Server {
    pub fn start(key: &Key, bundle: &Bundle) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port is free");
        let port = listener.local_addr().unwrap().port();
        let manifest = manifest(key, bundle, &format!("http://127.0.0.1:{port}/bundle"));
        let bytes = bundle.bytes.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let flag = stopping.clone();
        let thread = std::thread::spawn(move || {
            for stream in listener.incoming() {
                if flag.load(Ordering::SeqCst) {
                    break;
                }
                let mut stream = stream.expect("a loopback connection is accepted");
                // A request can arrive in several segments: read to the blank
                // line that ends its head.
                let mut request = [0u8; 4096];
                let mut n = 0;
                while !request[..n].windows(4).any(|w| w == b"\r\n\r\n") && n < request.len() {
                    let read = stream.read(&mut request[n..]).expect("the request is read");
                    if read == 0 {
                        break;
                    }
                    n += read;
                }
                let line = String::from_utf8_lossy(&request[..n]);
                let (status, body): (&str, &[u8]) = match line.split_whitespace().nth(1) {
                    Some("/latest.json") => ("200 OK", manifest.as_bytes()),
                    Some("/bundle") => ("200 OK", &bytes),
                    _ => ("404 Not Found", b""),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(head.as_bytes()).unwrap();
                stream.write_all(body).unwrap();
                stream.shutdown(Shutdown::Write).unwrap();
            }
        });
        Self {
            port,
            stopping,
            thread,
        }
    }

    pub fn endpoint(&self) -> String {
        format!("http://127.0.0.1:{}/latest.json", self.port)
    }

    pub fn stop(self) {
        self.stopping.store(true, Ordering::SeqCst);
        TcpStream::connect(("127.0.0.1", self.port)).expect("the server is listening");
        self.thread.join().expect("the server thread ends");
    }
}

/// The tray line for `update`: the running version, and, when a newer one is
/// available, the line that installs it.
pub fn line(update: &UpdateResult) -> (Option<&'static str>, String) {
    match &update.available {
        Some(available) => (
            Some(crate::tray::UPDATE_ID),
            format!(
                "Version {} \u{2014} Update to {available} available",
                update.current
            ),
        ),
        None => (None, format!("Version {}", update.current)),
    }
}

/// `update.check` as the tray uses it.
pub async fn check(control: &ControlHandle) -> Result<UpdateResult, RpcError> {
    let value = control.call(Method::UpdateCheck, None).await?;
    Ok(serde_json::from_value(value).expect("update.check matches its schema"))
}

/// Show what a check found. A computer that cannot reach the release
/// location (no internet) keeps showing what it showed before.
pub fn show(tray: &TrayController, result: Result<UpdateResult, RpcError>) {
    if let Ok(update) = result {
        tray.update_update(update);
    }
}

/// Ask `update.check` every [`POLL`] and show the answer; the first ask is now.
pub async fn poll(control: ControlHandle, tray: Arc<TrayController>) {
    let mut ticks = tokio::time::interval(POLL);
    loop {
        ticks.tick().await;
        show(&tray, check(&control).await);
    }
}

/// Install the offered release (the tray's "Update to Y" line).
pub async fn apply(control: &ControlHandle) -> Value {
    control
        .call(Method::UpdateApply, None)
        .await
        .expect("the offered release can be installed")
}
