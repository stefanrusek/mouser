//! Release fixtures for unit tests: a signer standing in for the Mouser
//! updater key, and a local HTTP server standing in for the release host.

use crate::lock::LockExt;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use blake2::{Blake2b512, Digest};
use ed25519_dalek::{Signer as _, SigningKey};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// A minisign key pair.
pub(crate) struct Signer {
    key: SigningKey,
    key_id: [u8; 8],
}

impl Signer {
    pub fn new(seed: u8) -> Self {
        Self {
            key: SigningKey::from_bytes(&[seed; 32]),
            key_id: [seed; 8],
        }
    }

    /// The base64 of the `.pub` file.
    pub fn pubkey(&self) -> String {
        let mut raw = b"Ed".to_vec();
        raw.extend(self.key_id);
        raw.extend(self.key.verifying_key().as_bytes());
        STANDARD.encode(format!(
            "untrusted comment: minisign public key\n{}\n",
            STANDARD.encode(raw)
        ))
    }

    /// The base64 of a `.sig` file for `data`; `prehashed` selects `ED`
    /// (what Tauri produces) over `Ed`.
    pub fn sign(&self, data: &[u8], prehashed: bool) -> String {
        let signature = if prehashed {
            self.key.sign(&Blake2b512::digest(data))
        } else {
            self.key.sign(data)
        };
        let comment = "timestamp:1 file:bundle";
        let mut raw = if prehashed {
            b"ED".to_vec()
        } else {
            b"Ed".to_vec()
        };
        raw.extend(self.key_id);
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

/// A local HTTP server answering `GET`s from a table of responses.
pub(crate) struct Server {
    pub port: u16,
    pub requests: Arc<Mutex<Vec<String>>>,
    routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>>,
}

impl Server {
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let routes: Arc<Mutex<BTreeMap<String, Vec<u8>>>> = Arc::default();
        let requests: Arc<Mutex<Vec<String>>> = Arc::default();
        let (table, seen) = (routes.clone(), requests.clone());
        tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let (table, seen) = (table.clone(), seen.clone());
                tokio::spawn(async move {
                    let mut buf = vec![0u8; 4096];
                    let n = stream.read(&mut buf).await.unwrap();
                    let head = String::from_utf8_lossy(&buf[..n]).to_string();
                    let path = head.split_whitespace().nth(1).unwrap().to_string();
                    seen.guard().push(path.clone());
                    let response = table.guard().get(&path).cloned();
                    let bytes = response.unwrap_or_else(|| {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    });
                    stream.write_all(&bytes).await.unwrap();
                    stream.shutdown().await.unwrap();
                });
            }
        });
        Self {
            port,
            requests,
            routes,
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    /// Answer `path` with 200 and `body`.
    pub fn serve(&self, path: &str, body: &[u8]) {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend(body);
        self.raw(path, &response);
    }

    /// Answer `path` with exactly these bytes.
    pub fn raw(&self, path: &str, response: &[u8]) {
        self.routes
            .guard()
            .insert(path.to_string(), response.to_vec());
    }

    /// Answer `path` with a redirect to `to`.
    pub fn redirect(&self, path: &str, to: &str) {
        self.raw(
            path,
            format!("HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        );
    }
}

impl Signer {
    /// Keep the key id but use another key (an impostor of this key).
    pub fn replace_key(&mut self, seed: u8) {
        self.key = SigningKey::from_bytes(&[seed; 32]);
    }
}

/// What `publish` put on the server, by platform key.
pub(crate) struct Published {
    pub bundles: BTreeMap<String, Vec<u8>>,
}

/// The platform keys of a manifest: every operating system on this host's
/// architecture.
pub(crate) fn platform_keys() -> Vec<String> {
    ["darwin", "windows", "linux"]
        .iter()
        .map(|os| format!("{os}-{}", std::env::consts::ARCH))
        .collect()
}

/// Publish release `version` on `server`: `latest.json` and one bundle per
/// platform, signed by `signer`, except that the bundles of `tampered`
/// platforms do not match their signatures.
pub(crate) fn publish(
    server: &Server,
    signer: &Signer,
    version: &str,
    tampered: &[&str],
) -> Published {
    let mut platforms = serde_json::Map::new();
    let mut bundles = BTreeMap::new();
    for key in platform_keys() {
        let bundle = format!("bundle {key} {version}").into_bytes();
        let path = format!("/{version}/{key}.bin");
        server.serve(&path, &bundle);
        let signed: &[u8] = if tampered.iter().any(|t| key.starts_with(t)) {
            b"something else"
        } else {
            &bundle
        };
        platforms.insert(
            key.clone(),
            serde_json::json!({"url": server.url(&path), "signature": signer.sign(signed, true)}),
        );
        bundles.insert(key, bundle);
    }
    let manifest = serde_json::json!({"version": version, "platforms": platforms});
    server.serve("/latest.json", manifest.to_string().as_bytes());
    Published { bundles }
}

/// An installer that keeps what it is given.
pub(crate) fn installer() -> (super::Installer, Arc<Mutex<Vec<super::Bundle>>>) {
    let installed: Arc<Mutex<Vec<super::Bundle>>> = Arc::default();
    let sink = installed.clone();
    let installer: super::Installer = Arc::new(move |bundle| {
        sink.guard().push(bundle);
        Ok(())
    });
    (installer, installed)
}

/// A computer running `version` whose release location is `server` and whose
/// updater key is `signer`'s.
pub(crate) async fn computer(
    lab: &crate::testing::Lab,
    name: &str,
    version: &str,
    automatic: bool,
    (server, signer): (&Server, &Signer),
    installer: super::Installer,
) -> crate::testing::TestNode {
    lab.start_released(
        name,
        |c| {
            c.updates.automatic = automatic;
            c.updates.release_url = server.url("/latest.json");
        },
        |p| {
            p.mouser_version = version.into();
            p.release_pubkey = signer.pubkey();
            p.installer = Some(installer);
        },
    )
    .await
}
