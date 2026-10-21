use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use mouser_proto::ids::ComputerId;
use mouser_proto::wire::{ControlMessage, MotionDatagram, PROTOCOL_VERSION};
use quinn::rustls::DigitallySignedStruct;
use quinn::rustls::client::danger::ServerCertVerifier;
use quinn::rustls::internal::msgs::codec::{Codec, Reader};
use quinn::rustls::pki_types::UnixTime;
use quinn::rustls::server::danger::ClientCertVerifier;

use super::*;

fn identity(n: u8) -> Identity {
    Identity::from_seed([n; 32])
}

fn loopback(t: &Transport) -> SocketAddr {
    SocketAddr::new(Ipv4Addr::LOCALHOST.into(), t.local_addr().port())
}

fn hello(id: ComputerId) -> ControlMessage {
    ControlMessage::Hello {
        protocol_version: PROTOCOL_VERSION,
        mouser_version: "0.1.0".into(),
        computer_id: id,
        display_name: "Test".into(),
        os: mouser_proto::model::Os::Linux,
        desk_id: None,
    }
}

/// Two bound transports with a connection from `a` (dialer) to `b`.
async fn connected() -> (
    Identity,
    Identity,
    Transport,
    Transport,
    Connection,
    Connection,
) {
    let (ia, ib) = (identity(1), identity(2));
    let (a, b) = (Transport::bind(&ia, 0), Transport::bind(&ib, 0));
    let dial = a.connect(loopback(&b), Some(ib.computer_id()));
    let (dialed, accepted) = tokio::join!(dial, async {
        Transport::complete(b.accept().await.unwrap())
            .await
            .unwrap()
    });
    (ia, ib, a, b, dialed.unwrap(), accepted)
}

#[tokio::test]
async fn binds_the_dual_stack_wildcard() {
    let t = Transport::bind(&identity(1), 0);
    assert!(t.local_addr().ip().is_unspecified());
    assert_ne!(t.local_addr().port(), 0);
}

#[tokio::test]
async fn falls_back_to_the_next_address_when_binding_fails() {
    let unassigned: IpAddr = "2001:db8::1".parse().unwrap();
    let t = Transport::bind_first(&identity(1), &[unassigned, Ipv4Addr::LOCALHOST.into()], 0);
    assert_eq!(t.local_addr().ip(), IpAddr::from(Ipv4Addr::LOCALHOST));
}

#[tokio::test]
async fn the_connection_speaks_the_mouser_alpn_and_identifies_both_peers() {
    let (ia, ib, _a, _b, dialed, accepted) = connected().await;
    let data = dialed.handshake_data().unwrap();
    let data = data
        .downcast::<quinn::crypto::rustls::HandshakeData>()
        .unwrap();
    assert_eq!(data.protocol.as_deref(), Some(ALPN));
    assert_eq!(peer_id(&dialed), ib.computer_id());
    assert_eq!(peer_id(&accepted), ia.computer_id());
}

#[tokio::test]
async fn a_peer_whose_key_is_not_the_expected_computer_is_rejected() {
    let (ia, ib) = (identity(1), identity(2));
    let (a, b) = (Transport::bind(&ia, 0), Transport::bind(&ib, 0));
    let wrong = ComputerId([0xee; 16]);
    let to = loopback(&b);
    let server = tokio::spawn(async move { Transport::complete(b.accept().await.unwrap()).await });
    let err = a.connect(to, Some(wrong)).await.unwrap_err();
    assert!(server.await.unwrap().is_err(), "the dialler hung up");
    assert!(err.to_string().contains("expected computer"), "{err}");
}

#[tokio::test]
async fn an_unknown_peer_can_be_dialled_without_an_expected_id() {
    let (ia, ib) = (identity(1), identity(2));
    let (a, b) = (Transport::bind(&ia, 0), Transport::bind(&ib, 0));
    let (dialed, _accepted) = tokio::join!(a.connect(loopback(&b), None), async {
        Transport::complete(b.accept().await.unwrap())
            .await
            .unwrap()
    });
    assert_eq!(peer_id(&dialed.unwrap()), ib.computer_id());
}

#[tokio::test]
async fn framed_messages_travel_both_ways_on_the_control_stream() {
    let (ia, ib, _a, _b, dialed, accepted) = connected().await;
    let mut dialer = Link::open(dialed).await;
    dialer.writer.send(&hello(ia.computer_id())).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(acceptor.peer, ia.computer_id());
    assert_eq!(dialer.peer, ib.computer_id());
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(hello(ia.computer_id()))
    );
    acceptor
        .writer
        .send(&hello(ib.computer_id()))
        .await
        .unwrap();
    acceptor.writer.send(&ControlMessage::Panic).await.unwrap();
    assert_eq!(
        dialer.reader.recv::<ControlMessage>().await,
        Some(hello(ib.computer_id()))
    );
    assert_eq!(
        dialer.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Panic)
    );
    acceptor.writer.finish().await;
    assert_eq!(dialer.reader.recv::<ControlMessage>().await, None);
}

#[tokio::test]
async fn datagrams_are_read_through_the_link() {
    let (ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let mut dialer = Link::open(dialed).await;
    dialer.writer.send(&hello(ia.computer_id())).await.unwrap();
    let acceptor = Link::accepted(accepted).await.unwrap();
    let motion = MotionDatagram {
        seq: 1,
        display_id_hash: 2,
        x: 3.0,
        y: 4.0,
    };
    dialer.send_datagram(&motion);
    assert_eq!(
        acceptor.recv_datagram::<MotionDatagram>().await,
        Some(motion)
    );
    acceptor.conn.close(0u32.into(), b"bye");
    assert_eq!(dialer.recv_datagram::<MotionDatagram>().await, None);
}

#[tokio::test]
async fn a_malformed_frame_is_fatal() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let (mut send, _recv) = dialed.open_bi().await.unwrap();
    send.write_all(&[0, 0, 0, 2, 0xff, 0xff]).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    let task = tokio::spawn(async move { acceptor.reader.recv::<ControlMessage>().await });
    assert!(task.await.unwrap_err().is_panic());
}

#[tokio::test]
async fn an_idle_connection_outlives_the_idle_timeout_thanks_to_keep_alive() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    tokio::time::sleep(MAX_IDLE + Duration::from_millis(800)).await;
    assert!(dialed.close_reason().is_none());
    assert!(accepted.close_reason().is_none());
}

#[test]
fn only_ed25519_certificates_identify_a_computer() {
    let other = rcgen::generate_simple_self_signed(vec!["x".into()]).unwrap();
    let err = public_key(other.cert.der()).unwrap_err();
    assert!(err.to_string().contains("Ed25519"), "{err}");
    let (cert, _) = certificate(&identity(1));
    assert_eq!(public_key(&cert).unwrap(), identity(1).public_key());
}

#[test]
fn tls12_signatures_are_verified_too() {
    let (cert, _) = certificate(&identity(1));
    let verifier = Pin::new(None);
    let mut wire = 0x0807u16.to_be_bytes().to_vec();
    wire.extend(64u16.to_be_bytes());
    wire.extend([0u8; 64]);
    let bad = DigitallySignedStruct::read(&mut Reader::init(&wire)).unwrap();
    assert!(ServerCertVerifier::verify_tls12_signature(&verifier, b"m", &cert, &bad).is_err());
    assert!(ClientCertVerifier::verify_tls12_signature(&verifier, b"m", &cert, &bad).is_err());
    assert!(ServerCertVerifier::verify_tls13_signature(&verifier, b"m", &cert, &bad).is_err());
    assert!(!ServerCertVerifier::supported_verify_schemes(&verifier).is_empty());
    assert!(!ClientCertVerifier::supported_verify_schemes(&verifier).is_empty());
}

#[test]
fn the_verifiers_accept_only_the_expected_computer() {
    let (cert, _) = certificate(&identity(1));
    let now = UnixTime::now();
    let name = quinn::rustls::pki_types::ServerName::try_from("mouser").unwrap();
    let matching = Pin::new(Some(identity(1).computer_id()));
    assert!(
        matching
            .verify_server_cert(&cert, &[], &name, &[], now)
            .is_ok()
    );
    let other = Pin::new(Some(identity(2).computer_id()));
    assert!(
        other
            .verify_server_cert(&cert, &[], &name, &[], now)
            .is_err()
    );
    assert!(ClientCertVerifier::verify_client_cert(&matching, &cert, &[], now).is_ok());
    assert!(ClientCertVerifier::root_hint_subjects(&matching).is_empty());
}
