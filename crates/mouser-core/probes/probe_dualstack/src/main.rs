//! Standalone probe for the dual-stack handshake loss on macOS; see README.md.
mod deliver;
mod ports;
mod quic;

use socket2::{Domain, Protocol, Socket, Type};
use std::net::{SocketAddr, UdpSocket};

pub fn sock(addr: &str, v6only: Option<bool>) -> std::io::Result<UdpSocket> {
    let addr: SocketAddr = addr.parse().unwrap();
    let s = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    if let Some(v) = v6only {
        s.set_only_v6(v)?;
    }
    s.bind(&addr.into())?;
    Ok(s.into())
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["ports"] => ports::run(2000),
        ["deliver", v4, target] => deliver::run(v4, target),
        ["listen", v4, secs] => deliver::listen(v4, secs.parse().unwrap()),
        ["quic", server, client, target, n, held] => quic::run(server, client, target, n.parse().unwrap(), held.parse().unwrap()).await,
        _ => eprintln!("usage: see README.md"),
    }
}
