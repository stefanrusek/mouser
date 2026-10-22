use std::sync::Arc;

use quinn::rustls::crypto::ring;
use quinn::rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
use quinn::rustls::{ClientConfig, RootCertStore, ServerConfig, version::TLS13};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use super::http::Http;
use super::testing::Server;

#[tokio::test]
async fn a_body_is_read_by_length_by_chunks_or_to_the_end() {
    let server = Server::start().await;
    server.serve("/plain", b"hello");
    server.raw(
        "/chunked",
        b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3;ext=1\r\nabc\r\n2\r\nde\r\n0\r\n\r\n",
    );
    server.raw(
        "/eof",
        b"HTTP/1.1 200 OK\r\nX-Other: 1\r\nnot a header\r\n\r\nrest",
    );
    let http = Http::system();
    assert_eq!(http.get(&server.url("/plain")).await.unwrap(), b"hello");
    assert_eq!(http.get(&server.url("/chunked")).await.unwrap(), b"abcde");
    assert_eq!(http.get(&server.url("/eof")).await.unwrap(), b"rest");
    assert_eq!(
        server.requests.lock().unwrap().as_slice(),
        ["/plain", "/chunked", "/eof"]
    );
}

#[tokio::test]
async fn redirects_are_followed_a_few_times() {
    let server = Server::start().await;
    server.serve("/final", b"done");
    server.redirect("/relative", "/final");
    server.redirect("/absolute", &server.url("/relative"));
    server.redirect("/loop", "/loop");
    let http = Http::system();
    assert_eq!(http.get(&server.url("/absolute")).await.unwrap(), b"done");
    assert_eq!(
        http.get(&server.url("/loop")).await.unwrap_err(),
        "too many redirects"
    );
}

#[tokio::test]
async fn failures_are_reported() {
    let server = Server::start().await;
    let http = Http::system();
    let missing = server.url("/missing");
    assert_eq!(
        http.get(&missing).await.unwrap_err(),
        format!("{missing} answered 404")
    );
    // A redirect without a destination is just an answer.
    server.raw(
        "/nowhere",
        b"HTTP/1.1 302 Found\r\nContent-Length: 0\r\n\r\n",
    );
    assert!(
        http.get(&server.url("/nowhere"))
            .await
            .unwrap_err()
            .contains("answered 302")
    );
    for (path, raw) in [
        ("/nohead", &b"HTTP/1.1 200 OK"[..]),
        ("/binary", b"HTTP/1.1 200 OK\xff\r\n\r\n"),
        ("/nostatus", b"HTTP/1.1\r\n\r\n"),
        ("/short", b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nabc"),
        (
            "/badchunk",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nzz\r\n",
        ),
        (
            "/cutchunk",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nab",
        ),
        (
            "/nosize",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5",
        ),
        (
            "/binarysize",
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\xff\r\n",
        ),
    ] {
        server.raw(path, raw);
        assert_eq!(
            http.get(&server.url(path)).await.unwrap_err(),
            "malformed HTTP response",
            "{path}"
        );
    }
    assert!(
        http.get("ftp://x")
            .await
            .unwrap_err()
            .contains("not an http")
    );
    let unused = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = unused.local_addr().unwrap().port();
    drop(unused);
    assert!(
        http.get(&format!("http://127.0.0.1:{port}/"))
            .await
            .unwrap_err()
            .contains("cannot reach")
    );
}

#[tokio::test]
async fn a_server_that_hangs_up_mid_request_is_reported() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        // Abort with a reset.
        socket2::SockRef::from(&stream)
            .set_linger(Some(std::time::Duration::ZERO))
            .unwrap();
    });
    let err = Http::system()
        .get(&format!("http://127.0.0.1:{port}/"))
        .await
        .unwrap_err();
    assert!(err.contains("dropped the connection"), "{err}");
}

fn tls_pair() -> (TlsAcceptor, Http) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let der = cert.cert.der().clone();
    let server = ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_protocol_versions(&[&TLS13])
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![der.clone()],
            PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into(),
        )
        .unwrap();
    let mut roots = RootCertStore::empty();
    roots.add(CertificateDer::from(der.to_vec())).unwrap();
    let client = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_protocol_versions(&[&TLS13])
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth();
    (
        TlsAcceptor::from(Arc::new(server)),
        Http::with_tls(Arc::new(client)),
    )
}

#[tokio::test]
async fn https_is_spoken_to_servers_the_client_trusts() {
    let (acceptor, http) = tls_pair();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (tcp, _) = listener.accept().await.unwrap();
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                let Ok(mut tls) = acceptor.accept(tcp).await else {
                    return;
                };
                let mut buf = [0u8; 1024];
                let _ = tls.read(&mut buf).await.unwrap();
                tls.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\n\r\nsecure")
                    .await
                    .unwrap();
                tls.shutdown().await.unwrap();
            });
        }
    });
    assert_eq!(
        http.get(&format!("https://localhost:{port}/"))
            .await
            .unwrap(),
        b"secure"
    );
    // The platform's trust store does not know the test certificate.
    let err = Http::system()
        .get(&format!("https://localhost:{port}/"))
        .await
        .unwrap_err();
    assert!(
        err.contains("secure connection to localhost failed"),
        "{err}"
    );
    // An IP address is not a valid name for the test certificate either.
    let err = http
        .get(&format!("https://127.0.0.1:{port}/"))
        .await
        .unwrap_err();
    assert!(err.contains("secure connection"), "{err}");
    // Nor is a name that cannot be one.
    let err = http.get("https://bad host/").await.unwrap_err();
    assert!(err.contains("not a valid host name"), "{err}");
}
