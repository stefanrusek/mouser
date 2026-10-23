//! QUIC handshakes between fresh endpoints, as the loopback tests do them.
//! Each failure is checked against the kernel's socket table: does another
//! socket hold the failed endpoint's port?
use crate::sock;
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::rustls::{
    self, DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime},
};
use quinn::{ClientConfig, Endpoint, EndpointConfig, ServerConfig, TokioRuntime, TransportConfig};
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
struct AnyCert;
impl ServerCertVerifier for AnyCert {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(&self, _: &[u8], _: &CertificateDer<'_>, _: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider().signature_verification_algorithms.supported_schemes()
    }
}

fn transport() -> Arc<TransportConfig> {
    let mut t = TransportConfig::default();
    t.max_idle_timeout(Some(Duration::from_secs(1).try_into().unwrap()));
    Arc::new(t)
}

pub fn server_config() -> ServerConfig {
    let cert = rcgen::generate_simple_self_signed(vec!["mouser".into()]).unwrap();
    let key = PrivateKeyDer::Pkcs8(cert.signing_key.serialize_der().into());
    let tls = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13]).unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert.cert.der().clone()], key).unwrap();
    let mut s = ServerConfig::with_crypto(Arc::new(QuicServerConfig::try_from(tls).unwrap()));
    s.transport_config(transport());
    s
}

pub fn client_config() -> ClientConfig {
    let tls = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13]).unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AnyCert))
        .with_no_client_auth();
    let mut c = ClientConfig::new(Arc::new(QuicClientConfig::try_from(tls).unwrap()));
    c.transport_config(transport());
    c
}

/// `dual` = `[::]` with IPV6_V6ONLY off (what Mouser binds), `v6only`, or an IP.
pub fn bind(kind: &str, port: u16) -> UdpSocket {
    match kind {
        "dual" => sock(&format!("[::]:{port}"), Some(false)),
        "v6only" => sock(&format!("[::]:{port}"), Some(true)),
        ip if ip.contains(':') => sock(&format!("[{ip}]:{port}"), Some(true)),
        ip => sock(&format!("{ip}:{port}"), None),
    }
    .unwrap()
}

pub fn endpoint(socket: UdpSocket, server: Option<ServerConfig>) -> Endpoint {
    Endpoint::new(EndpointConfig::default(), server, socket, Arc::new(TokioRuntime)).unwrap()
}

/// Other sockets (any process) bound on `port`, from `netstat`.
pub fn sharers(port: u16) -> Vec<String> {
    let out = std::process::Command::new("netstat").args(["-an", "-p", "udp"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.split_whitespace().nth(3).is_some_and(|a| a.ends_with(&format!(".{port}"))))
        .map(|l| l.split_whitespace().take(4).collect::<Vec<_>>().join(" "))
        .collect()
}

pub async fn run(server: &str, client: &str, target: &str, n: usize, held: usize) {
    // IPv4 sockets held open, as a busy test run (or other programs) holds them.
    let _held: Vec<_> = (0..held).map(|_| sock("127.0.0.1:0", None).unwrap()).collect();
    let (mut ok, mut failed, mut explained) = (0, 0, 0);
    for i in 0..n {
        let s = endpoint(bind(server, 0), Some(server_config()));
        let c = endpoint(bind(client, 0), None);
        let port = s.local_addr().unwrap().port();
        let cport = c.local_addr().unwrap().port();
        // The sockets on both ports at bind time: a short-lived socket of another
        // program can be gone again by the time the handshake has timed out.
        let at_bind = (sharers(port), sharers(cport));
        let addr: SocketAddr = if target.contains(':') { format!("[{target}]:{port}") } else { format!("{target}:{port}") }.parse().unwrap();
        let accept = {
            let s = s.clone();
            tokio::spawn(async move {
                if let Some(inc) = s.accept().await { let _ = inc.await; }
            })
        };
        let r = c.connect_with(client_config(), addr, "mouser").unwrap().await;
        match r {
            Ok(conn) => { ok += 1; conn.close(0u32.into(), b""); }
            Err(e) => {
                failed += 1;
                let (ss, cs) = (sharers(port), sharers(cport));
                // The endpoint's own socket is one line; more means a sharer.
                if ss.len() > 1 || cs.len() > 1 || at_bind.0.len() > 1 || at_bind.1.len() > 1 { explained += 1; }
                println!("#{i} {e}: server :{port} {ss:?} (at bind {:?}); client :{cport} {cs:?} (at bind {:?})", at_bind.0, at_bind.1);
            }
        }
        accept.abort();
        s.close(0u32.into(), b""); c.close(0u32.into(), b"");
    }
    println!("server {server}, client {client}, dial {target}, {held} IPv4 sockets held: {ok} ok, {failed} failed ({explained} with a port shared with another socket) of {n}");
}
