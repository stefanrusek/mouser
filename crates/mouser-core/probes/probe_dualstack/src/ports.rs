//! Does the kernel hand an ephemeral port to a socket when a socket of the
//! other family already holds it?
use crate::sock;
use std::collections::HashSet;

fn pair(first: (&str, Option<bool>), second: (&str, Option<bool>), n: usize) {
    let a: Vec<_> = (0..n).map(|_| sock(first.0, first.1).unwrap()).collect();
    let ports: HashSet<u16> = a.iter().map(|s| s.local_addr().unwrap().port()).collect();
    let mut keep = vec![];
    let mut hits = 0;
    for _ in 0..n {
        let s = sock(second.0, second.1).unwrap();
        hits += ports.contains(&s.local_addr().unwrap().port()) as usize;
        keep.push(s);
    }
    println!("{n} x {first:?} held, then {n} x {second:?}: {hits} given a held port");
}

fn explicit(first: (&str, Option<bool>), second: (&str, Option<bool>)) {
    let a = sock(first.0, first.1).unwrap();
    let p = a.local_addr().unwrap().port();
    let addr = second.0.replace(":0", &format!(":{p}"));
    println!("{first:?} holds {p}; explicit bind {addr} {:?}: {:?}", second.1, sock(&addr, second.1).map(|s| s.local_addr().unwrap()));
}

pub fn run(n: usize) {
    let dual = ("[::]:0", Some(false));
    let v6only = ("[::]:0", Some(true));
    for v4 in ["0.0.0.0:0", "127.0.0.1:0"] {
        pair((v4, None), dual, n);
        pair(dual, (v4, None), n);
        pair((v4, None), v6only, n);
        pair(v6only, (v4, None), n);
        explicit((v4, None), dual);
        explicit(dual, (v4, None));
    }
}
