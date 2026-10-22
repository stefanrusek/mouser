//! Network change notifications (US3): rtnetlink link and address events,
//! and logind's `PrepareForSleep(false)` when the computer wakes (its sockets
//! may be dead even when no address changed).

use crate::bus::{block_on, connect_system, spawn};
use futures_util::StreamExt;
use mouser_platform::WatchHandle;
use rustix::event::{PollFd, PollFlags, poll};
use rustix::net::netlink::SocketAddrNetlink;
use rustix::net::{AddressFamily, RecvFlags, SocketType, bind, recv, socket};
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::os::unix::net::UnixStream;
use std::sync::mpsc::Sender;
use zbus::message::Type;
use zbus::{MatchRule, MessageStream};

const RTMGRP_LINK: u32 = 1;
const RTMGRP_IPV4_IFADDR: u32 = 0x10;
const RTMGRP_IPV6_IFADDR: u32 = 0x100;

const RTM_NEWLINK: u16 = 16;
const RTM_DELLINK: u16 = 17;
const RTM_NEWADDR: u16 = 20;
const RTM_DELADDR: u16 = 21;

/// `sizeof(struct nlmsghdr)`.
const HEADER: usize = 16;

/// The routing socket subscribed to link and address changes.
pub(crate) fn open_netlink() -> OwnedFd {
    let fd = socket(AddressFamily::NETLINK, SocketType::RAW, None).expect("open a netlink socket");
    bind(
        &fd,
        &SocketAddrNetlink::new(0, RTMGRP_LINK | RTMGRP_IPV4_IFADDR | RTMGRP_IPV6_IFADDR),
    )
    .expect("subscribe to network changes");
    fd
}

/// Whether a datagram carries a link or address message.
pub(crate) fn relevant(datagram: &[u8]) -> bool {
    let mut at = 0;
    while at + HEADER <= datagram.len() {
        let length = u32::from_ne_bytes(datagram[at..at + 4].try_into().unwrap()) as usize;
        let kind = u16::from_ne_bytes(datagram[at + 4..at + 6].try_into().unwrap());
        if matches!(kind, RTM_NEWLINK | RTM_DELLINK | RTM_NEWADDR | RTM_DELADDR) {
            return true;
        }
        if length < HEADER {
            return false;
        }
        at += (length + 3) & !3;
    }
    false
}

/// Fires `tx` for each relevant datagram on `source` until the handle drops.
pub(crate) fn watch_netlink(source: OwnedFd, tx: Sender<()>) -> WatchHandle {
    let (stop_reader, stop_writer) = UnixStream::pair().expect("stop pair");
    let thread = std::thread::spawn(move || {
        let mut buffer = vec![0u8; 32 * 1024];
        loop {
            let mut fds = [
                PollFd::new(&source, PollFlags::IN),
                PollFd::new(&stop_reader, PollFlags::IN),
            ];
            poll(&mut fds, None).expect("poll the network watch");
            if !fds[1].revents().is_empty() {
                return;
            }
            let (read, _) = recv(&source, &mut buffer, RecvFlags::empty()).expect("read netlink");
            if relevant(&buffer[..read]) {
                let _ = tx.send(());
            }
        }
    });
    WatchHandle::new(move || {
        drop(stop_writer);
        thread.join().unwrap();
    })
}

/// logind's resume signal, subscribed before this returns; `None` when the
/// computer has no system bus.
async fn resume_signals(env: &HashMap<String, String>) -> Option<MessageStream> {
    let conn = connect_system(env).await.ok()?;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.login1")
        .unwrap()
        .interface("org.freedesktop.login1.Manager")
        .unwrap()
        .member("PrepareForSleep")
        .unwrap()
        .build();
    Some(
        MessageStream::for_match_rule(rule, &conn, None)
            .await
            .expect("subscribe to logind"),
    )
}

/// Fires `tx` for each `PrepareForSleep(false)`.
async fn forward_resumes(mut signals: MessageStream, tx: Sender<()>) {
    while let Some(signal) = signals.next().await {
        let sleeping: bool = signal.unwrap().body().deserialize().unwrap();
        if !sleeping {
            let _ = tx.send(());
        }
    }
}

/// Fires `tx` when an interface or address changes and when the computer
/// wakes from sleep, until the returned handle is dropped.
pub(crate) fn watch(env: &HashMap<String, String>, tx: Sender<()>) -> WatchHandle {
    let env = env.clone();
    let resumes = block_on(async move { resume_signals(&env).await });
    let resumes = resumes.map(|signals| spawn(forward_resumes(signals, tx.clone())));
    let netlink = watch_netlink(open_netlink(), tx);
    WatchHandle::new(move || {
        if let Some(resumes) = resumes {
            resumes.abort();
        }
        drop(netlink);
    })
}
