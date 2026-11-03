//! T547: a member's recorded version follows what it says when it
//! reconnects, published by one desk-mate only. Its name stays the desk's.

use crate::lock::LockExt;
use crate::testing::*;
use mouser_proto::ids::ComputerId;

fn recorded(node: &TestNode, who: ComputerId) -> String {
    let core = node.engine.ctx.core.guard();
    let m = core
        .state
        .desk
        .as_ref()
        .unwrap()
        .state
        .members
        .iter()
        .find(|m| m.computer_id == who)
        .unwrap()
        .clone();
    m.mouser_version
}

#[tokio::test]
async fn a_member_that_reconnects_with_another_version_is_recorded_by_one_desk_mate() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let gamma = nodes[2].engine.computer_id();
    let truth = recorded(&nodes[2], gamma);
    for node in &nodes {
        let mut core = node.engine.ctx.core.guard();
        let m = core
            .state
            .desk
            .as_mut()
            .unwrap()
            .state
            .members
            .iter_mut()
            .find(|m| m.computer_id == gamma)
            .unwrap();
        m.mouser_version = "0.2.4".into();
    }
    let before = epoch_of(&nodes[0]);
    // Gamma's links drop; the desk-mates dial it again.
    let conns: Vec<_> = nodes[2]
        .engine
        .ctx
        .core
        .guard()
        .peers
        .values()
        .map(|p| p.conn.clone())
        .collect();
    for conn in conns {
        conn.close(0u32.into(), b"done");
    }
    for node in &nodes {
        eventually("the refreshed entry everywhere", || async {
            recorded(node, gamma) == truth
        })
        .await;
    }
    settle().await;
    let lowest = nodes[0]
        .engine
        .computer_id()
        .min(nodes[1].engine.computer_id());
    for node in &nodes {
        let core = node.engine.ctx.core.guard();
        let state = &core.state.desk.as_ref().unwrap().state;
        assert_eq!((state.epoch, state.author), (before + 1, lowest));
    }
}
