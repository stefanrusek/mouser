//! Collecting the desk's logs (data-model "Collection states"): the
//! requester asks only computers that can answer, and records every desk
//! computer, with the reason for any it has no logs of.

use std::time::{Duration, Instant};

use mouser_proto::control::{BundleReason, ConnectionState};
use mouser_proto::ids::ComputerId;
use serde_json::Value;

use super::collect::Plan;
use crate::lock::LockExt;
use crate::testing::*;

fn plan_of(node: &TestNode, peer: &TestNode) -> Plan {
    let id = peer.engine.computer_id();
    let core = node.engine.ctx.core.guard();
    node.engine
        .ctx
        .logs_targets(&core)
        .into_iter()
        .find(|t| t.id == id)
        .expect("a desk member")
        .plan
}

async fn desk(versions: &[(&str, &str, bool)]) -> (Lab, Vec<TestNode>) {
    let lab = Lab::new();
    let mut nodes = Vec::new();
    for (name, version, logging) in versions {
        nodes.push(lab.start_version(name, version, *logging).await);
    }
    let nodes = desk_from(nodes).await;
    (lab, nodes)
}

fn lines(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8(bytes.to_vec())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[tokio::test]
async fn only_connected_computers_of_0_3_0_or_later_are_asked() {
    let (_lab, mut nodes) = desk(&[
        ("Alpha", "0.3.0", true),
        ("Beta", "0.3.0", true),
        ("Gamma", "0.2.8", true),
        ("Delta", "0.3.1", true),
    ])
    .await;
    let delta = nodes.pop().unwrap();
    let delta_id = delta.engine.computer_id();
    delta.engine.shutdown().await;
    let (a, b, g) = (&nodes[0], &nodes[1], &nodes[2]);
    eventually("Alpha to see Delta gone", || async {
        !a.engine.ctx.core.guard().peers.contains_key(&delta_id)
    })
    .await;
    assert_eq!(plan_of(a, b), Plan::Ask);
    // A computer below 0.3.0 would panic at an unknown message: never asked.
    assert_eq!(plan_of(a, g), Plan::Skip(BundleReason::OlderVersion));
    let core = a.engine.ctx.core.guard();
    let targets = a.engine.ctx.logs_targets(&core);
    let delta = targets.iter().find(|t| t.id == delta_id).unwrap();
    assert_eq!(delta.plan, Plan::Skip(BundleReason::Offline));
    assert_eq!(delta.version, "0.3.1");
    // Nobody is listed twice, and the requester is not a target.
    assert_eq!(targets.len(), 3);
}

#[tokio::test]
async fn every_computer_is_recorded_requester_first_with_what_it_sent_or_why_not() {
    let (_lab, mut nodes) = desk(&[
        ("Alpha", "0.3.0", true),
        ("Beta", "0.3.0", true),
        ("Gamma", "0.2.8", true),
        ("Delta", "0.3.0", true),
    ])
    .await;
    let delta = nodes.pop().unwrap();
    let delta_id = delta.engine.computer_id();
    delta.engine.shutdown().await;
    let (a, b, g) = (&nodes[0], &nodes[1], &nodes[2]);
    eventually("Alpha to see Delta gone", || async {
        !a.engine.ctx.core.guard().peers.contains_key(&delta_id)
    })
    .await;
    let computers = a.engine.ctx.collect_logs().await;
    let ids: Vec<ComputerId> = computers.iter().map(|c| c.id).collect();
    assert_eq!(ids[0], a.engine.computer_id(), "the requester is first");
    assert_eq!(ids.len(), 4);

    let mine = &computers[0];
    assert!(mine.included);
    assert_eq!(mine.connection, ConnectionState::SelfComputer);
    assert_eq!(mine.files[0].0, "mouser.log");
    assert_eq!(lines(&mine.files[0].1)[0]["event"], "log.header");

    let beta = computers
        .iter()
        .find(|c| c.id == b.engine.computer_id())
        .unwrap();
    assert!(beta.included);
    assert_eq!(beta.reason, None);
    assert_eq!(beta.connection, ConnectionState::Connected);
    assert_eq!(beta.name, "Beta");
    assert_eq!(beta.version, "0.3.0");
    assert_eq!(beta.mode, Some(mouser_proto::wire::ComputerMode::Installed));
    assert!(beta.os_version.is_some());
    assert_eq!(
        beta.config_toml.as_deref(),
        Some(
            std::fs::read_to_string(b.dir.path().join("config.toml"))
                .unwrap()
                .as_str()
        )
    );
    let first = &lines(&beta.files[0].1)[0];
    assert_eq!(first["event"], "log.header");
    assert_eq!(
        first["fields"]["computer_id"],
        b.engine.computer_id().to_string()
    );

    // Below 0.3.0: never requested, its card built from what is known.
    let gamma = computers
        .iter()
        .find(|c| c.id == g.engine.computer_id())
        .unwrap();
    assert!(!gamma.included);
    assert_eq!(gamma.reason, Some(BundleReason::OlderVersion));
    assert_eq!(gamma.version, "0.2.8");
    assert_eq!(gamma.connection, ConnectionState::Connected);
    assert!(gamma.files.is_empty() && gamma.config_toml.is_none());
    assert_eq!(gamma.mode, None);
    assert_eq!(gamma.os_version, None);

    let delta = computers.iter().find(|c| c.id == delta_id).unwrap();
    assert!(!delta.included);
    assert_eq!(delta.reason, Some(BundleReason::Offline));
    assert!(delta.files.is_empty());
}

#[tokio::test]
async fn a_computer_with_logging_off_has_its_card_and_settings_but_no_logs() {
    let (_lab, nodes) = desk(&[("Alpha", "0.3.0", true), ("Beta", "0.3.0", false)]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let computers = a.engine.ctx.collect_logs().await;
    let beta = &computers[1];
    assert_eq!(beta.id, b.engine.computer_id());
    assert!(!beta.included);
    assert_eq!(beta.reason, Some(BundleReason::LoggingOff));
    assert!(beta.files.is_empty());
    assert!(
        beta.config_toml
            .as_deref()
            .unwrap()
            .contains("[diagnostics]")
    );
    assert_eq!(beta.name, "Beta");
}

#[tokio::test]
async fn a_link_lost_before_the_answer_is_offline_and_nothing_waits_on_a_timer() {
    let lab = Lab::simulated();
    let mut nodes = Vec::new();
    for name in ["Alpha", "Beta"] {
        nodes.push(lab.start_version(name, "0.3.0", true).await);
    }
    let nodes = desk_from(nodes).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    // Beta falls silent: the request goes out on a link that is still up,
    // and the answer never comes. The link is lost after the idle timeout,
    // and that ends the wait.
    b.sim.as_ref().unwrap().set_blackhole(true);
    let asked = Instant::now();
    let computers = a.engine.ctx.collect_logs().await;
    assert!(
        asked.elapsed() < Duration::from_secs(10),
        "{:?}",
        asked.elapsed()
    );
    assert_eq!(computers[1].reason, Some(BundleReason::Offline));
    assert!(!computers[1].included);
    assert!(a.engine.ctx.logs_waiters.guard().is_empty());
}
