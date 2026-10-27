//! QUIC transport (research R1, R2): one endpoint per computer, ALPN
//! `mouser/1`, a self-signed certificate over the device key, and peers
//! verified by pinning the certificate key to the expected `ComputerId`.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use mouser_proto::ids::{ComputerId, DevicePublicKey};
use mouser_proto::wire::{FrameError, decode_frame, encode_frame};
use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
use quinn::rustls::client::danger::{
    HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
};
use quinn::rustls::crypto::{CryptoProvider, WebPkiSupportedAlgorithms, ring};
use quinn::rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use quinn::rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use quinn::rustls::{
    ClientConfig, DigitallySignedStruct, DistinguishedName, Error, ServerConfig, SignatureScheme,
};
use quinn::{
    Connection, ConnectionError, Endpoint, EndpointConfig, Incoming, RecvStream, SendStream,
    TokioRuntime, TransportConfig,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use socket2::{Domain, Protocol, Socket, Type};

use crate::identity::Identity;

/// Application protocol negotiated in the TLS handshake.
pub const ALPN: &[u8] = b"mouser/1";
/// Research R2: keep-alive ping interval.
pub const KEEP_ALIVE: Duration = Duration::from_millis(750);
/// Research R2: a path that is silent this long is declared lost.
pub const MAX_IDLE: Duration = Duration::from_secs(3);

/// SPKI prefix of an Ed25519 public key; the 32 key bytes follow.
const ED25519_SPKI: [u8; 12] = [
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];
/// PKCS#8 prefix of an Ed25519 private key; the 32 seed bytes follow.
const ED25519_PKCS8: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

fn provider() -> Arc<CryptoProvider> {
    Arc::new(ring::default_provider())
}

/// The Ed25519 key of a certificate.
fn public_key(cert: &CertificateDer<'_>) -> Result<DevicePublicKey, Error> {
    let der: &[u8] = cert;
    let at = der
        .windows(ED25519_SPKI.len())
        .position(|w| w == ED25519_SPKI)
        .ok_or_else(|| Error::General("certificate does not hold an Ed25519 key".into()))?
        + ED25519_SPKI.len();
    let key = der
        .get(at..at + 32)
        .expect("certificate holds the whole key");
    Ok(DevicePublicKey(key.try_into().expect("32 bytes")))
}

/// Self-signed certificate over the device key, and the key's PKCS#8 encoding.
fn certificate(identity: &Identity) -> (CertificateDer<'static>, Vec<u8>) {
    let mut pkcs8 = ED25519_PKCS8.to_vec();
    pkcs8.extend(identity.seed());
    let pair = rcgen::KeyPair::from_pkcs8_der_and_sign_algo(
        &PrivatePkcs8KeyDer::from(pkcs8.clone()),
        &rcgen::PKCS_ED25519,
    )
    .expect("Ed25519 key pair");
    let params = rcgen::CertificateParams::new(vec!["mouser".to_string()]).expect("valid names");
    let cert = params.self_signed(&pair).expect("self-signed certificate");
    (cert.der().clone(), pkcs8)
}

/// Verifies that a peer's certificate key is the expected computer's (no CA).
#[derive(Debug)]
struct Pin {
    expected: Option<ComputerId>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl Pin {
    fn new(expected: Option<ComputerId>) -> Self {
        Self {
            expected,
            algorithms: provider().signature_verification_algorithms,
        }
    }

    fn check(&self, cert: &CertificateDer<'_>) -> Result<(), Error> {
        let id = ComputerId::from_public_key(&public_key(cert)?);
        match self.expected {
            Some(expected) if expected != id => Err(Error::General(format!(
                "certificate is not the expected computer {expected}"
            ))),
            _ => Ok(()),
        }
    }
}

impl ServerCertVerifier for Pin {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        self.check(end_entity)
            .map(|()| ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        quinn::rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        quinn::rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for Pin {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, Error> {
        self.check(end_entity)
            .map(|()| ClientCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        quinn::rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        quinn::rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// How much may be unacknowledged on a path before sending waits (bytes).
/// Pointer motion and keystrokes are tiny packets sent for as long as the
/// user types; with the default window of a dozen packets a path that is
/// silent for two seconds fills it, and nothing more is sent until the
/// exponentially backed-off probe timer fires, which is later than the 3 s
/// idle timeout: the session would end though the path was back after 2 s.
/// With room for everything typed in the idle timeout, the first packet
/// sent after the path returns is acknowledged at once.
const WINDOW: u64 = 1 << 20;

fn transport_config() -> Arc<TransportConfig> {
    let mut config = TransportConfig::default();
    let mut cubic = quinn::congestion::CubicConfig::default();
    cubic.initial_window(WINDOW);
    config.congestion_controller_factory(Arc::new(cubic));

    config.keep_alive_interval(Some(KEEP_ALIVE));
    config.max_idle_timeout(Some(MAX_IDLE.try_into().expect("idle timeout fits")));
    Arc::new(config)
}

/// One UDP socket bound at `addr`; an IPv6 one carries IPv6 only (IPv4 has
/// a socket of its own, see [`socket`]).
fn udp_socket(addr: SocketAddr) -> std::io::Result<std::net::UdpSocket> {
    let socket = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    if addr.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.bind(&addr.into())?;
    Ok(socket.into())
}

/// One computer's QUIC endpoint.
pub struct Transport {
    endpoint: Endpoint,
    /// The bound sockets, kept so the endpoint can be moved onto other
    /// handles of them: the configured port never changes.
    sockets: socket::Sockets,
    certificate: CertificateDer<'static>,
    pkcs8: Vec<u8>,
}

impl Transport {
    /// Bind `0.0.0.0:port` and, where IPv6 can be had, `[::]:port`; the
    /// endpoint then speaks both (see [`socket`]). Panics saying why if the
    /// port cannot be bound.
    pub fn bind(identity: &Identity, port: u16) -> Self {
        let sockets =
            socket::Sockets::bind(Ipv4Addr::UNSPECIFIED, Some(Ipv6Addr::UNSPECIFIED), port)
                .unwrap_or_else(|error| panic!("a UDP socket can be bound: {error}"));
        let (certificate, pkcs8) = certificate(identity);
        let mut tls = ServerConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&quinn::rustls::version::TLS13])
            .expect("TLS 1.3 is supported")
            .with_client_cert_verifier(Arc::new(Pin::new(None)))
            .with_single_cert(vec![certificate.clone()], key(&pkcs8))
            .expect("valid certificate");
        tls.alpn_protocols = vec![ALPN.to_vec()];
        let mut server = quinn::ServerConfig::with_crypto(Arc::new(
            QuicServerConfig::try_from(tls).expect("QUIC-capable TLS"),
        ));
        server.transport_config(transport_config());
        let endpoint = Endpoint::new_with_abstract_socket(
            EndpointConfig::default(),
            Some(server),
            sockets.quinn(),
            Arc::new(TokioRuntime),
        )
        .expect("QUIC endpoint");
        Self {
            endpoint,
            sockets,
            certificate,
            pkcs8,
        }
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.endpoint.local_addr().expect("bound endpoint")
    }

    /// Dial `addr`; with `expected`, fail unless the peer is that computer.
    pub async fn connect(
        &self,
        addr: SocketAddr,
        expected: Option<ComputerId>,
    ) -> Result<Connection, ConnectionError> {
        let mut tls = ClientConfig::builder_with_provider(provider())
            .with_protocol_versions(&[&quinn::rustls::version::TLS13])
            .expect("TLS 1.3 is supported")
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pin::new(expected)))
            .with_client_auth_cert(vec![self.certificate.clone()], key(&self.pkcs8))
            .expect("valid certificate");
        tls.alpn_protocols = vec![ALPN.to_vec()];
        let mut client = quinn::ClientConfig::new(Arc::new(
            QuicClientConfig::try_from(tls).expect("QUIC-capable TLS"),
        ));
        client.transport_config(transport_config());
        self.endpoint
            .connect_with(client, addr, "mouser")
            .expect("valid dial parameters")
            .await
    }

    /// The next incoming connection attempt; `None` once the endpoint is closed.
    pub async fn accept(&self) -> Option<Incoming> {
        self.endpoint.accept().await
    }

    /// Finish the handshake of an incoming attempt.
    pub async fn complete(incoming: Incoming) -> Result<Connection, ConnectionError> {
        incoming.await
    }

    /// Stop accepting and close every connection.
    pub fn close(&self) {
        self.endpoint.close(0u32.into(), b"shutdown");
    }

    /// Wait until every connection has been closed.
    pub async fn wait_idle(&self) {
        self.endpoint.wait_idle().await;
    }
}

fn key(pkcs8: &[u8]) -> quinn::rustls::pki_types::PrivateKeyDer<'static> {
    PrivatePkcs8KeyDer::from(pkcs8.to_vec()).into()
}

/// The computer on the other end, from its certificate.
pub fn peer_id(conn: &Connection) -> ComputerId {
    let certs = conn
        .peer_identity()
        .expect("TLS identity present")
        .downcast::<Vec<CertificateDer<'static>>>()
        .expect("rustls certificate chain");
    let key = public_key(certs.first().expect("peer certificate")).expect("verified certificate");
    ComputerId::from_public_key(&key)
}

/// Writes length-prefixed postcard frames.
pub struct FrameWriter(SendStream);

impl FrameWriter {
    pub async fn send<T: Serialize>(&mut self, msg: &T) -> Result<(), quinn::WriteError> {
        self.0.write_all(&encode_frame(msg)).await
    }

    /// Finish the stream and wait until the peer has received everything.
    pub async fn finish(&mut self) {
        let _ = self.0.finish();
        let _ = self.0.stopped().await;
    }
}

/// Reads length-prefixed postcard frames.
pub struct FrameReader {
    recv: RecvStream,
    buf: Vec<u8>,
}

impl FrameReader {
    /// The next message; `None` once the stream ended or the connection was lost.
    pub async fn recv<T: DeserializeOwned>(&mut self) -> Option<T> {
        loop {
            match decode_frame::<T>(&self.buf) {
                Ok((msg, used)) => {
                    self.buf.drain(..used);
                    return Some(msg);
                }
                Err(FrameError::Incomplete) => {}
                // v2 placeholder: T102/T103
                Err(FrameError::Malformed | FrameError::UnknownVariant(_)) => {
                    panic!("malformed frame from peer")
                }
            }
            let chunk = self.recv.read_chunk(usize::MAX, true).await.ok()??;
            self.buf.extend_from_slice(&chunk.bytes);
        }
    }
}

/// One connection with its control stream.
pub struct Link {
    pub conn: Connection,
    pub peer: ComputerId,
    pub writer: FrameWriter,
    pub reader: FrameReader,
}

impl Link {
    fn new(conn: Connection, send: SendStream, recv: RecvStream) -> Self {
        Self {
            peer: peer_id(&conn),
            conn,
            writer: FrameWriter(send),
            reader: FrameReader {
                recv,
                buf: Vec::new(),
            },
        }
    }

    /// Open the control stream on a connection we dialled. The peer sees the
    /// stream once the first frame is written. `None` if the connection has
    /// already ended: the peer can hang up the moment it is reached (it was
    /// shutting down).
    pub async fn open(conn: Connection) -> Option<Self> {
        let (send, recv) = conn.open_bi().await.ok()?;
        Some(Self::new(conn, send, recv))
    }

    /// Accept the control stream on a connection we accepted; `None` if the
    /// connection ends first.
    pub async fn accepted(conn: Connection) -> Option<Self> {
        let (send, recv) = conn.accept_bi().await.ok()?;
        Some(Self::new(conn, send, recv))
    }

    /// Send one unreliable datagram.
    pub fn send_datagram<T: Serialize>(&self, msg: &T) {
        let bytes = postcard::to_stdvec(msg).expect("datagram serializes");
        self.conn
            .send_datagram(Bytes::from(bytes))
            .expect("datagram fits and the connection is open");
    }

    /// The next datagram; `None` once the connection is closed.
    pub async fn recv_datagram<T: DeserializeOwned>(&self) -> Option<T> {
        let bytes = self.conn.read_datagram().await.ok()?;
        Some(postcard::from_bytes(&bytes).expect("well-formed datagram"))
    }
}

pub mod netchange;
mod socket;

#[cfg(test)]
mod netchange_tests;
#[cfg(test)]
mod socket_tests;
#[cfg(test)]
mod tests;
