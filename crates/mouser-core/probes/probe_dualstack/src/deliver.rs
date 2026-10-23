//! Two sockets share a port: an IPv4 one and a dual-stack one. Which of them
//! gets an IPv4 datagram sent to that port?
use crate::sock;
use std::time::Duration;

pub fn run(v4: &str, target: &str) {
    // Ephemeral dual-stack sockets until one lands on a port an IPv4 socket holds.
    let holders: Vec<_> = (0..2000).map(|_| sock(&format!("{v4}:0"), None).unwrap()).collect();
    let ports: std::collections::HashMap<u16, usize> = holders
        .iter()
        .enumerate()
        .map(|(i, s)| (s.local_addr().unwrap().port(), i))
        .collect();
    let mut tries = 0;
    let (dual, holder) = loop {
        tries += 1;
        let d = sock("[::]:0", Some(false)).unwrap();
        if let Some(i) = ports.get(&d.local_addr().unwrap().port()) {
            break (d, &holders[*i]);
        }
    };
    let port = dual.local_addr().unwrap().port();
    println!("after {tries} dual-stack binds, [::]:{port} shares its port with {}", holder.local_addr().unwrap());
    for s in [&dual, holder] {
        s.set_read_timeout(Some(Duration::from_millis(300))).unwrap();
    }
    let tx = sock("0.0.0.0:0", None).unwrap();
    let (mut to_dual, mut to_holder) = (0, 0);
    for _ in 0..100 {
        tx.send_to(b"x", format!("{target}:{port}")).unwrap();
        let mut b = [0u8; 8];
        std::thread::sleep(Duration::from_millis(2));
        if dual.recv_from(&mut b).is_ok() { to_dual += 1; }
        holder.set_nonblocking(true).unwrap();
        if holder.recv_from(&mut b).is_ok() { to_holder += 1; }
    }
    println!("100 datagrams to {target}:{port}: dual-stack socket got {to_dual}, IPv4 socket got {to_holder}");
    let tx6 = sock("[::]:0", Some(true)).unwrap();
    let mut got = 0;
    for _ in 0..20 {
        tx6.send_to(b"x", format!("[::1]:{port}")).unwrap();
        let mut b = [0u8; 8];
        if dual.recv_from(&mut b).is_ok() { got += 1; }
    }
    println!("20 datagrams to [::1]:{port}: dual-stack socket got {got}");
}

/// Make the collision, then count for `secs` which socket gets the datagrams
/// another computer sends to the port.
pub fn listen(v4: &str, secs: u64) {
    let holders: Vec<_> = (0..2000).map(|_| sock(&format!("{v4}:0"), None).unwrap()).collect();
    let ports: std::collections::HashMap<u16, usize> =
        holders.iter().enumerate().map(|(i, s)| (s.local_addr().unwrap().port(), i)).collect();
    let (dual, holder) = loop {
        let d = sock("[::]:0", Some(false)).unwrap();
        if let Some(i) = ports.get(&d.local_addr().unwrap().port()) {
            break (d, &holders[*i]);
        }
    };
    let port = dual.local_addr().unwrap().port();
    println!("listening: [::]:{port} and {} share the port", holder.local_addr().unwrap());
    dual.set_nonblocking(true).unwrap();
    holder.set_nonblocking(true).unwrap();
    let (mut d, mut h) = (0, 0);
    let end = std::time::Instant::now() + Duration::from_secs(secs);
    let mut b = [0u8; 64];
    while std::time::Instant::now() < end {
        while let Ok((_, from)) = dual.recv_from(&mut b) { d += 1; if d == 1 { println!("dual-stack got one from {from}"); } }
        while let Ok((_, from)) = holder.recv_from(&mut b) { h += 1; if h == 1 { println!("IPv4 socket got one from {from}"); } }
        std::thread::sleep(Duration::from_millis(5));
    }
    println!("in {secs} s: dual-stack socket got {d}, IPv4 socket got {h}");
}
