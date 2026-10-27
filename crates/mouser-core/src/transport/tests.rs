use std::net::{Ipv4Addr, SocketAddr};
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
    let (dialed, accepted) =
        crate::testing::handshake(&a, &b, loopback(&b), Some(ib.computer_id())).await;
    (ia, ib, a, b, dialed, accepted)
}

#[tokio::test]
async fn binds_both_wildcards_on_one_port() {
    let t = Transport::bind(&identity(1), 0);
    assert!(t.local_addr().ip().is_unspecified());
    let port = t.local_addr().port();
    assert_ne!(port, 0);
    assert!(std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_err());
    assert!(super::udp_socket(SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, port))).is_err());
}

#[tokio::test]
async fn ipv6_peers_are_reached_too() {
    let (ia, ib) = (identity(1), identity(2));
    let (a, b) = (Transport::bind(&ia, 0), Transport::bind(&ib, 0));
    let to = SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, b.local_addr().port()));
    let (dialed, accepted) = crate::testing::handshake(&a, &b, to, Some(ib.computer_id())).await;
    assert_eq!(dialed.remote_address(), to);
    assert!(accepted.remote_address().is_ipv6());
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
    let server = tokio::time::timeout(Duration::from_secs(30), server)
        .await
        .expect("the attempt reached the accepting side");
    assert!(server.unwrap().is_err(), "the dialler hung up");
    assert!(err.to_string().contains("expected computer"), "{err}");
}

#[tokio::test]
async fn an_unknown_peer_can_be_dialled_without_an_expected_id() {
    let (ia, ib) = (identity(1), identity(2));
    let (a, b) = (Transport::bind(&ia, 0), Transport::bind(&ib, 0));
    let (dialed, _accepted) = crate::testing::handshake(&a, &b, loopback(&b), None).await;
    assert_eq!(peer_id(&dialed), ib.computer_id());
}

#[tokio::test]
async fn framed_messages_travel_both_ways_on_the_control_stream() {
    let (ia, ib, _a, _b, dialed, accepted) = connected().await;
    let mut dialer = Link::open(dialed).await.unwrap();
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

/// A computer can hang up the moment the connection is up (it is shutting
/// down): there is then no control stream to open, and no link.
#[tokio::test]
async fn a_connection_closed_before_its_control_stream_opens_is_no_link() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    accepted.close(0u32.into(), b"shutdown");
    dialed.closed().await;
    assert!(Link::open(dialed).await.is_none());
}

#[tokio::test]
async fn datagrams_are_read_through_the_link() {
    let (ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let mut dialer = Link::open(dialed).await.unwrap();
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
async fn an_unknown_variant_is_skipped_and_the_link_stays_up() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let (mut send, _recv) = dialed.open_bi().await.unwrap();
    // A frame whose message index no build of this version knows, then a
    // known message.
    send.write_all(&[0, 0, 0, 1, 0x7f]).await.unwrap();
    send.write_all(&encode_frame(&ControlMessage::Panic))
        .await
        .unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Panic)
    );
    assert!(acceptor.conn.close_reason().is_none());
}

#[tokio::test]
async fn a_malformed_frame_closes_the_link_with_reason_protocol() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let (mut send, _recv) = dialed.open_bi().await.unwrap();
    send.write_all(&[0, 0, 0, 2, 0xff, 0xff]).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(acceptor.reader.recv::<ControlMessage>().await, None);
    match dialed.closed().await {
        ConnectionError::ApplicationClosed(close) => assert_eq!(&close.reason[..], b"protocol"),
        other => panic!("closed by the peer with a reason, not {other:?}"),
    }
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

/// Take a free port on both wildcard addresses.
pub(super) fn hold_wildcard_port() -> (std::net::UdpSocket, std::net::UdpSocket) {
    let v4 = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let port = v4.local_addr().unwrap().port();
    let v6 = super::udp_socket(SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, port))).unwrap();
    (v4, v6)
}

#[tokio::test]
#[should_panic(expected = "a UDP socket can be bound: ")]
async fn binding_a_port_that_is_taken_panics_with_the_reason() {
    let held = hold_wildcard_port();
    let id = crate::identity::Identity::from_seed([4; 32]);
    super::Transport::bind(&id, held.0.local_addr().unwrap().port());
}

/// Regression: an endpoint that is closed and dropped gives its port back
/// within a scheduler tick, so the same port binds again at once, every
/// time (a restart on the fixed port must not fail spuriously).
#[tokio::test]
async fn a_port_binds_again_right_after_its_endpoint_is_closed() {
    let id = crate::identity::Identity::from_seed([3; 32]);
    let probe = std::net::UdpSocket::bind(("::", 0)).unwrap();
    let port = probe.local_addr().unwrap().port();
    drop(probe);
    for _ in 0..200 {
        let endpoint = super::Transport::bind(&id, port);
        assert_eq!(endpoint.local_addr().port(), port);
        endpoint.close();
        endpoint.wait_idle().await;
        drop(endpoint);
        // The endpoint's driver lets go of the socket on its next turn.
        let mut turns = 0;
        while std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port)).is_err()
            || super::udp_socket(SocketAddr::from((std::net::Ipv6Addr::UNSPECIFIED, port))).is_err()
        {
            turns += 1;
            assert!(turns < 100, "the port was never released");
            tokio::task::yield_now().await;
        }
    }
}

/// Regression: on macOS an IPv6 socket's port is checked against IPv6 sockets
/// only, so a dual-stack `[::]:p` bound although an IPv4 socket held
/// `0.0.0.0:p`, and every IPv4 datagram to the port went to that socket: the
/// endpoint looked fine and never heard an IPv4 peer. A port held on IPv4
/// alone is a port that cannot be bound.
#[tokio::test]
#[should_panic(expected = "a UDP socket can be bound: ")]
async fn a_port_held_on_ipv4_alone_cannot_be_bound() {
    let held = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).unwrap();
    let id = crate::identity::Identity::from_seed([5; 32]);
    super::Transport::bind(&id, held.local_addr().unwrap().port());
}

/// Regression: on macOS an ephemeral dual-stack `[::]:0` was handed ports
/// that IPv4 sockets held (about 1 in 7 with 2000 held), and the IPv4
/// datagrams of a whole handshake then went to the other socket (the loopback
/// tests' intermittent handshake timeouts). The endpoint's IPv4 port is never
/// one another IPv4 socket holds.
#[tokio::test]
async fn an_ephemeral_endpoint_never_shares_its_ipv4_port() {
    let held: Vec<_> = (0..150)
        .map(|_| std::net::UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap())
        .collect();
    let ports: std::collections::HashSet<u16> = held
        .iter()
        .map(|s| s.local_addr().unwrap().port())
        .collect();
    let id = crate::identity::Identity::from_seed([6; 32]);
    for _ in 0..1000 {
        let t = super::Transport::bind(&id, 0);
        assert!(
            !ports.contains(&t.local_addr().port()),
            "{}",
            t.local_addr()
        );
    }
}

#[tokio::test]
async fn a_skipped_frame_tells_the_hook_its_variant_index() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let (mut send, _recv) = dialed.open_bi().await.unwrap();
    send.write_all(&[0, 0, 0, 1, 0x7f]).await.unwrap();
    send.write_all(&encode_frame(&ControlMessage::Panic))
        .await
        .unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    let skipped = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = skipped.clone();
    acceptor
        .reader
        .on_unknown(move |variant| seen.lock().unwrap().push(variant));
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Panic)
    );
    assert_eq!(*skipped.lock().unwrap(), [0x7f]);
    assert!(!acceptor.reader.undecodable());
}

#[tokio::test]
async fn a_reader_that_closed_for_a_malformed_frame_says_so() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let (mut send, _recv) = dialed.open_bi().await.unwrap();
    send.write_all(&[0, 0, 0, 2, 0xff, 0xff]).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(acceptor.reader.recv::<ControlMessage>().await, None);
    assert!(acceptor.reader.undecodable());
}

#[tokio::test]
async fn a_skipped_unknown_variant_is_logged_as_wire_unknown_with_the_peer() {
    use crate::testing::{Lab, Rogue, logged};
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let rogue = Rogue::new(0x75);
    let (mut link, _) = rogue.dial(&node, None).await;
    link.writer.send_raw(&[0, 0, 0, 1, 0x7f]).await;
    let id = rogue.identity.computer_id().to_string();
    let record = logged(&node, "wire.unknown", |r| r["peer"] == id.as_str()).await;
    assert_eq!(record["level"], "warn");
    assert_eq!(record["fields"], serde_json::json!({"variant": 127}));
    // The link stays up.
    assert!(link.conn.close_reason().is_none());
}

fn legacy_status() -> mouser_proto::wire::PeerStatus {
    use mouser_proto::control::{Problem, ProblemKind};
    mouser_proto::wire::PeerStatus {
        permissions: Default::default(),
        limitations: vec!["a limitation".into()],
        secure_input_holder: None,
        problems: vec![Problem {
            problem_id: "secure_input:x".into(),
            kind: ProblemKind::SecureInput,
            computer: ComputerId([4; 16]),
            message: "text".into(),
            app: None,
            fix: None,
            raised_at: "2026-01-01T00:00:00.000Z".into(),
            cleared_at: None,
        }],
        mode: mouser_proto::wire::ComputerMode::Portable,
    }
}

#[tokio::test]
async fn a_reader_reads_a_001_status_from_a_0_2_x_peer_and_the_frames_after_it() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let (mut send, _recv) = dialed.open_bi().await.unwrap();
    let status = legacy_status();
    let mut bytes = mouser_proto::wire::encode_status_v1(&status);
    bytes.extend(mouser_proto::wire::encode_frame(&ControlMessage::Panic));
    send.write_all(&bytes).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Status(status))
    );
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Panic)
    );
    assert!(!acceptor.reader.undecodable());
}

#[tokio::test]
async fn a_writer_sends_a_status_in_the_001_shape_on_request() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let mut dialer = Link::open(dialed).await.unwrap();
    let status = legacy_status();
    dialer.writer.send_legacy_status(&status).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Status(status))
    );
}

#[tokio::test]
async fn a_reader_still_reads_every_other_message_and_a_status_in_the_current_shape() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let mut dialer = Link::open(dialed).await.unwrap();
    dialer.writer.send(&ControlMessage::Panic).await.unwrap();
    let status = legacy_status();
    dialer
        .writer
        .send(&ControlMessage::Status(status.clone()))
        .await
        .unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Panic)
    );
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Status(status))
    );
}

#[tokio::test]
async fn a_001_status_without_problems_is_read_too() {
    let (_ia, _ib, _a, _b, dialed, accepted) = connected().await;
    let mut dialer = Link::open(dialed).await.unwrap();
    let mut status = legacy_status();
    status.problems.clear();
    dialer.writer.send_legacy_status(&status).await.unwrap();
    let mut acceptor = Link::accepted(accepted).await.unwrap();
    assert_eq!(
        acceptor.reader.recv::<ControlMessage>().await,
        Some(ControlMessage::Status(status))
    );
}
