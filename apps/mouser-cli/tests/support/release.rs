//! A release host for the acceptance suites: a local HTTP server serving
//! `latest.json` and signed bundles, and the key that signs them (as in
//! mouser-core's acceptance suites). Nothing here touches the real network.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use blake2::{Blake2b512, Digest};
use ed25519_dalek::{Signer as _, SigningKey};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Stands in for the Mouser updater key pair.
pub struct Signer {
    key: SigningKey,
}

impl Signer {
    pub fn new() -> Self {
        Self {
            key: SigningKey::from_bytes(&[42; 32]),
        }
    }

    /// The base64 of the minisign `.pub` file.
    pub fn pubkey(&self) -> String {
        let mut raw = b"Ed".to_vec();
        raw.extend([42u8; 8]);
        raw.extend(self.key.verifying_key().as_bytes());
        STANDARD.encode(format!(
            "untrusted comment: minisign public key\n{}\n",
            STANDARD.encode(raw)
        ))
    }

    /// The base64 of the minisign `.sig` file of `data`, as Tauri writes it.
    pub fn sign(&self, data: &[u8]) -> String {
        let signature = self.key.sign(&Blake2b512::digest(data));
        let comment = "timestamp:1 file:bundle";
        let mut raw = b"ED".to_vec();
        raw.extend([42u8; 8]);
        raw.extend(signature.to_bytes());
        let mut signed = signature.to_bytes().to_vec();
        signed.extend(comment.as_bytes());
        let global = self.key.sign(&signed);
        STANDARD.encode(format!(
            "untrusted comment: signature\n{}\ntrusted comment: {comment}\n{}\n",
            STANDARD.encode(raw),
            STANDARD.encode(global.to_bytes())
        ))
    }
}

/// The release host.
pub struct Host {
    port: u16,
    routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
}

impl Host {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Arc::default();
        let table = routes.clone();
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let table = table.clone();
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = stream.read(&mut buf).await.unwrap();
                    let head = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = head.split_whitespace().nth(1).unwrap().to_string();
                    let body = table.lock().unwrap().get(&path).cloned();
                    let mut response = match &body {
                        Some(body) => format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes(),
                        None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec(),
                    };
                    response.extend(body.unwrap_or_default());
                    stream.write_all(&response).await.unwrap();
                    stream.shutdown().await.unwrap();
                });
            }
        });
        Self { port, routes }
    }

    /// The `updates.release_url` that leads to the published `latest.json`.
    pub fn release_url(&self) -> String {
        format!("http://127.0.0.1:{}/latest.json", self.port)
    }

    /// Publish `version`: a bundle for every operating system on this
    /// host's architecture, signed by `signer`. Returns the bundles by
    /// platform key.
    pub fn publish(&self, signer: &Signer, version: &str) -> BTreeMap<String, Vec<u8>> {
        let mut platforms = serde_json::Map::new();
        let mut bundles = BTreeMap::new();
        for os in ["darwin", "windows", "linux"] {
            let key = format!("{os}-{}", std::env::consts::ARCH);
            let bundle = format!("Mouser {version} for {key}").into_bytes();
            let path = format!("/{version}/{key}");
            self.routes
                .lock()
                .unwrap()
                .insert(path.clone(), bundle.clone());
            platforms.insert(
                key.clone(),
                serde_json::json!({
                    "url": format!("http://127.0.0.1:{}{path}", self.port),
                    "signature": signer.sign(&bundle),
                }),
            );
            bundles.insert(key, bundle);
        }
        let manifest = serde_json::json!({"version": version, "platforms": platforms});
        self.routes
            .lock()
            .unwrap()
            .insert("/latest.json".into(), manifest.to_string().into_bytes());
        bundles
    }
}
