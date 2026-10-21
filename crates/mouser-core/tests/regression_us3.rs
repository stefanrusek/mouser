//! Regression tests for Story 3 (constitution VI): each hypothesis for why
//! ShareMouse dropped connections (research.md "Plan-Phase Handoff
//! disposition") is reproduced here and must not end the session.

mod steps;
mod support;

use std::time::Duration;

use mouser_core::discovery::advertised_addrs;
use mouser_platform::{InterfaceKind, NetInterface, SecureInputHolder};
use mouser_proto::control::ProblemKind;
use steps::us3::{LinkWorld, Typing};

fn virtual_interface(name: &str, addr: [u8; 4]) -> NetInterface {
    NetInterface {
        name: name.into(),
        kind: InterfaceKind::Virtual,
        addrs: vec![std::net::IpAddr::from(addr)],
    }
}

/// A VPN, a virtual machine network and a container network on both
/// computers while one types on the other: the addresses Mouser announces
/// are the physical LAN's only, and typing is not interrupted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn advertising_over_a_virtual_interface_does_not_end_the_session() {
    let mut w = LinkWorld::new();
    w.alpha_drives_beta().await;
    let (keyboard, network) = (w.platform("Alpha"), w.sim("Beta"));
    w.typing = Some(Typing::start(keyboard, network));
    tokio::time::sleep(Duration::from_millis(500)).await;
    for name in ["Alpha", "Beta"] {
        let sim = w.sim(name);
        for (adapter, addr) in [
            ("utun3", [10, 8, 0, 2]),
            ("vmnet8", [192, 168, 64, 1]),
            ("docker0", [172, 17, 0, 1]),
        ] {
            sim.add_interface(virtual_interface(adapter, addr));
        }
        let announced = advertised_addrs(&sim.interfaces());
        assert!(!announced.is_empty());
        for adapter in sim
            .interfaces()
            .iter()
            .filter(|i| i.kind == InterfaceKind::Virtual)
        {
            assert!(
                adapter.addrs.iter().all(|a| !announced.contains(a)),
                "{name} would announce its {} address",
                adapter.name
            );
        }
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    for name in ["Alpha", "Beta"] {
        assert!(w.sim(name).remove_interface("utun3"));
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    w.finish_typing_and_check().await;
    w.lab.shutdown().await;
}

/// The path is gone for two seconds in the middle of typing (a Wi-Fi
/// roam, a VPN re-routing): shorter than the 3 s idle timeout, so the
/// session survives and every keystroke still arrives, in order.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_two_second_path_flap_while_typing_does_not_end_the_session() {
    let mut w = LinkWorld::new();
    w.alpha_drives_beta().await;
    let (keyboard, network) = (w.platform("Alpha"), w.sim("Beta"));
    w.typing = Some(Typing::start(keyboard, network));
    tokio::time::sleep(Duration::from_secs(1)).await;
    let beta = w.sim("Beta");
    beta.set_blackhole(true);
    tokio::time::sleep(Duration::from_secs(2)).await;
    beta.set_blackhole(false);
    tokio::time::sleep(Duration::from_secs(2)).await;
    w.finish_typing_and_check().await;
    w.lab.shutdown().await;
}

/// An application on the receiving Mac holds Secure Input while the other
/// computer types: that is a problem to show, not a reason to drop the
/// connection.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn secure_input_on_the_receiver_does_not_end_the_session() {
    let mut w = LinkWorld::new();
    w.alpha_drives_beta().await;
    let (keyboard, network) = (w.platform("Alpha"), w.sim("Beta"));
    w.typing = Some(Typing::start(keyboard, network));
    tokio::time::sleep(Duration::from_secs(1)).await;
    w.platform("Beta").set_secure_input(Some(SecureInputHolder {
        pid: 412,
        app_name: "Password Manager".into(),
    }));
    // Alpha hears of it and tells its user.
    let beta_id = w.lab.node("Beta").id();
    w.lab
        .node("Alpha")
        .wait_status("the Secure Input problem", |s| {
            s.problems
                .iter()
                .any(|p| p.kind == ProblemKind::SecureInput && p.computer == beta_id)
        })
        .await;
    // It is held for longer than the idle timeout.
    tokio::time::sleep(Duration::from_secs(4)).await;
    w.finish_typing_and_check().await;
    w.lab.shutdown().await;
}
