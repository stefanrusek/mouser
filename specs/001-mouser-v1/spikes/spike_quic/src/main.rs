//! Throwaway probe for Mouser plan phase. NOT production code.
//!
//! Modes:
//!   bench    <server_ip> <client_ip>   bidi stream + datagram echo, p50/p99 RTT
//!   migrate  <server_ip> <client_ip_a> <client_ip_b>
//!                                      Endpoint::rebind mid-stream, check continuity
//!   pathloss <keepalive_ms> <idle_ms> <blackhole_ms>
//!                                      UDP relay black-holes the path; measure detection
use anyhow::{anyhow, Result};
use bytes::Bytes;
use quinn::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use quinn::{ClientConfig, Connection, Endpoint, IdleTimeout, ServerConfig, TransportConfig};
use std::net::{IpAddr, SocketAddr, UdpSocket as StdUdp};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn certs() -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let ck = rcgen::generate_simple_self_signed(vec!["localhost".into()])?;
    let cert = ck.cert.der().clone();
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(ck.signing_key.serialize_der()));
    Ok((cert, key))
}

fn transport(keepalive: Option<Duration>, idle: Duration) -> Arc<TransportConfig> {
    let mut t = TransportConfig::default();
    t.keep_alive_interval(keepalive);
    t.max_idle_timeout(Some(IdleTimeout::try_from(idle).unwrap()));
    Arc::new(t)
}

fn make_server(bind: SocketAddr, t: Arc<TransportConfig>) -> Result<(Endpoint, CertificateDer<'static>)> {
    let (cert, key) = certs()?;
    let mut sc = ServerConfig::with_single_cert(vec![cert.clone()], key)?;
    sc.transport_config(t);
    // migration(true) is the default; set explicitly for the record.
    sc.migration(true);
    Ok((Endpoint::server(sc, bind)?, cert))
}

fn make_client(bind: SocketAddr, cert: CertificateDer<'static>, t: Arc<TransportConfig>) -> Result<Endpoint> {
    let mut roots = quinn::rustls::RootCertStore::empty();
    roots.add(cert)?;
    let mut cc = ClientConfig::with_root_certificates(Arc::new(roots))?;
    cc.transport_config(t);
    let mut ep = Endpoint::client(bind)?;
    ep.set_default_client_config(cc);
    Ok(ep)
}

/// Server: echo datagrams; echo bi streams (length-delimited 32-byte frames); log remote addr changes.
fn spawn_echo_server(ep: Endpoint) {
    tokio::spawn(async move {
        while let Some(inc) = ep.accept().await {
            tokio::spawn(async move {
                let conn = match inc.await {
                    Ok(c) => c,
                    Err(e) => return eprintln!("[server] handshake failed: {e}"),
                };
                eprintln!("[server] accepted from {}", conn.remote_address());
                let c2 = conn.clone();
                tokio::spawn(async move {
                    let mut last = c2.remote_address();
                    loop {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        if c2.close_reason().is_some() { break; }
                        let now = c2.remote_address();
                        if now != last {
                            eprintln!("[server] remote address changed {last} -> {now}");
                            last = now;
                        }
                    }
                });
                let c3 = conn.clone();
                tokio::spawn(async move {
                    while let Ok((mut s, mut r)) = c3.accept_bi().await {
                        tokio::spawn(async move {
                            let mut buf = [0u8; 32];
                            while r.read_exact(&mut buf).await.is_ok() {
                                if s.write_all(&buf).await.is_err() { break; }
                            }
                        });
                    }
                });
                loop {
                    match conn.read_datagram().await {
                        Ok(d) => { let _ = conn.send_datagram(d); }
                        Err(e) => { eprintln!("[server] connection ended: {e}"); break; }
                    }
                }
            });
        }
    });
}

fn pct(mut v: Vec<Duration>) -> String {
    v.sort();
    let p = |q: f64| v[((v.len() as f64 - 1.0) * q).round() as usize];
    format!("n={} min={:?} p50={:?} p99={:?} max={:?}", v.len(), v[0], p(0.5), p(0.99), v[v.len() - 1])
}

async fn dgram_rtt(conn: &Connection, n: usize) -> (Vec<Duration>, usize) {
    let mut out = Vec::with_capacity(n);
    let mut lost = 0;
    for i in 0..n {
        let mut payload = [0u8; 32];
        payload[..8].copy_from_slice(&(i as u64).to_le_bytes());
        let t0 = Instant::now();
        conn.send_datagram(Bytes::copy_from_slice(&payload)).unwrap();
        loop {
            match tokio::time::timeout(Duration::from_millis(500), conn.read_datagram()).await {
                Ok(Ok(d)) if d[..8] == payload[..8] => { out.push(t0.elapsed()); break; }
                Ok(Ok(_)) => continue, // stale echo
                Ok(Err(e)) => panic!("conn err {e}"),
                Err(_) => { lost += 1; break; }
            }
        }
    }
    (out, lost)
}

async fn bench(server_ip: IpAddr, client_ip: IpAddr) -> Result<()> {
    let t = transport(Some(Duration::from_secs(1)), Duration::from_secs(5));
    let (sep, cert) = make_server(SocketAddr::new(server_ip, 0), t.clone())?;
    let saddr = sep.local_addr()?;
    spawn_echo_server(sep);
    let cep = make_client(SocketAddr::new(client_ip, 0), cert, t)?;
    let t0 = Instant::now();
    let conn = cep.connect(saddr, "localhost")?.await?;
    println!("connected {} -> {} handshake={:?} max_datagram_size={:?}",
        cep.local_addr()?, conn.remote_address(), t0.elapsed(), conn.max_datagram_size());

    // warm-up
    let _ = dgram_rtt(&conn, 50).await;
    let (d, lost) = dgram_rtt(&conn, 1000).await;
    println!("datagram 32B echo: {} lost={lost}", pct(d));

    let (mut s, mut r) = conn.open_bi().await?;
    let mut v = Vec::new();
    let mut buf = [0u8; 32];
    for i in 0..1050u64 {
        let mut p = [0u8; 32];
        p[..8].copy_from_slice(&i.to_le_bytes());
        let t0 = Instant::now();
        s.write_all(&p).await?;
        r.read_exact(&mut buf).await?;
        assert_eq!(buf, p);
        if i >= 50 { v.push(t0.elapsed()); }
    }
    println!("bidi stream 32B echo: {}", pct(v));
    println!("quinn path rtt estimate: {:?}", conn.stats().path.rtt);
    conn.close(0u32.into(), b"done");
    cep.wait_idle().await;
    Ok(())
}

async fn migrate(server_ip: IpAddr, a: IpAddr, b: IpAddr) -> Result<()> {
    let t = transport(Some(Duration::from_millis(500)), Duration::from_secs(3));
    let (sep, cert) = make_server(SocketAddr::new(server_ip, 0), t.clone())?;
    let saddr = sep.local_addr()?;
    spawn_echo_server(sep);
    let cep = make_client(SocketAddr::new(a, 0), cert, t)?;
    let conn = cep.connect(saddr, "localhost")?.await?;
    println!("connected client_local={} server={}", cep.local_addr()?, saddr);
    let (mut s, mut r) = conn.open_bi().await?;
    let mut buf = [0u8; 32];
    let mut stream_rtts = Vec::new();
    let mut dg_ok = 0;
    let mut dg_lost = 0;
    for i in 0..600u64 {
        if i == 300 {
            let sock = StdUdp::bind(SocketAddr::new(b, 0))?;
            let new_local = sock.local_addr()?;
            let t0 = Instant::now();
            cep.rebind(sock)?;
            println!("REBIND at msg {i}: client_local now {} (rebind call took {:?})", new_local, t0.elapsed());
        }
        let mut p = [0u8; 32];
        p[..8].copy_from_slice(&i.to_le_bytes());
        let t0 = Instant::now();
        s.write_all(&p).await?;
        tokio::time::timeout(Duration::from_secs(2), r.read_exact(&mut buf)).await
            .map_err(|_| anyhow!("stream echo timeout at {i}"))??;
        assert_eq!(buf, p);
        let rtt = t0.elapsed();
        if (298..=303).contains(&i) { println!("  stream msg {i} rtt={rtt:?}"); }
        stream_rtts.push(rtt);
        let (d, l) = dgram_rtt(&conn, 1).await;
        dg_ok += d.len();
        dg_lost += l;
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    println!("after 600 msgs: stream all echoed in order; datagrams ok={dg_ok} lost={dg_lost}");
    println!("stream rtt overall: {}", pct(stream_rtts));
    println!("conn still open: close_reason={:?}; stats.path.rtt={:?}", conn.close_reason(), conn.stats().path.rtt);
    tokio::time::sleep(Duration::from_millis(100)).await;
    conn.close(0u32.into(), b"done");
    cep.wait_idle().await;
    Ok(())
}

/// UDP relay client<->server that can be told to drop everything.
async fn relay(server: SocketAddr, drop_all: Arc<AtomicBool>) -> Result<SocketAddr> {
    let front = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await?);
    let back = Arc::new(tokio::net::UdpSocket::bind("127.0.0.1:0").await?);
    back.connect(server).await?;
    let addr = front.local_addr()?;
    let client_addr = Arc::new(tokio::sync::Mutex::new(None::<SocketAddr>));
    {
        let (front, back, d, ca) = (front.clone(), back.clone(), drop_all.clone(), client_addr.clone());
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            while let Ok((n, from)) = front.recv_from(&mut buf).await {
                *ca.lock().await = Some(from);
                if !d.load(Ordering::Relaxed) { let _ = back.send(&buf[..n]).await; }
            }
        });
    }
    {
        let (front, back, d, ca) = (front, back, drop_all, client_addr);
        tokio::spawn(async move {
            let mut buf = vec![0u8; 65536];
            while let Ok(n) = back.recv(&mut buf).await {
                if d.load(Ordering::Relaxed) { continue; }
                if let Some(c) = *ca.lock().await { let _ = front.send_to(&buf[..n], c).await; }
            }
        });
    }
    Ok(addr)
}

async fn pathloss(ka_ms: u64, idle_ms: u64, bh_ms: u64) -> Result<()> {
    let t = transport(Some(Duration::from_millis(ka_ms)), Duration::from_millis(idle_ms));
    let (sep, cert) = make_server("127.0.0.1:0".parse()?, t.clone())?;
    let saddr = sep.local_addr()?;
    spawn_echo_server(sep);
    let drop_all = Arc::new(AtomicBool::new(false));
    let raddr = relay(saddr, drop_all.clone()).await?;
    let cep = make_client("127.0.0.1:0".parse()?, cert, t)?;
    let conn = cep.connect(raddr, "localhost")?.await?;
    println!("keep_alive_interval={ka_ms}ms max_idle_timeout={idle_ms}ms blackhole={bh_ms}ms; connected via relay");
    // idle (no app traffic) for 2s to show keep-alive keeps it up
    tokio::time::sleep(Duration::from_millis(2000)).await;
    println!("after 2s idle with keep-alive: close_reason={:?}", conn.close_reason());
    let t0 = Instant::now();
    drop_all.store(true, Ordering::Relaxed);
    println!("BLACKHOLE ON");
    let d2 = drop_all.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(bh_ms)).await;
        d2.store(false, Ordering::Relaxed);
        println!("BLACKHOLE OFF at {:?}", t0.elapsed());
    });
    match tokio::time::timeout(Duration::from_millis(bh_ms + idle_ms * 3 + 2000), conn.closed()).await {
        Ok(e) => println!("client detected loss after {:?}: {e}", t0.elapsed()),
        Err(_) => {
            let (d, l) = dgram_rtt(&conn, 5).await;
            println!("connection SURVIVED blackhole; post-recovery datagram echoes ok={} lost={l}", d.len());
        }
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("bench") => bench(a[2].parse()?, a[3].parse()?).await,
        Some("migrate") => migrate(a[2].parse()?, a[3].parse()?, a[4].parse()?).await,
        Some("pathloss") => pathloss(a[2].parse()?, a[3].parse()?, a[4].parse()?).await,
        _ => Err(anyhow!("usage: bench|migrate|pathloss ...")),
    }
}
