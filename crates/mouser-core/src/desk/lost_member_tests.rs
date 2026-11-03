//! T549: a computer that was neither removed nor departed stays a member
//! when two desk states cross.

use crate::lock::LockExt;
use crate::state::{FILE, LocalState};
use crate::testing::*;
use mouser_proto::ids::ComputerId;

fn members_of(node: &TestNode) -> Vec<ComputerId> {
    let core = node.engine.ctx.core.guard();
    let mut ids: Vec<_> = core
        .state
        .desk
        .as_ref()
        .unwrap()
        .state
        .members
        .iter()
        .map(|m| m.computer_id)
        .collect();
    ids.sort();
    ids
}

/// Four members; the fourth is offline while the others admit a fifth; it
/// comes back with a desk state of its own whose epoch is higher.
#[tokio::test]
async fn a_member_admitted_while_one_was_offline_survives_that_ones_older_state() {
    let lab = Lab::new();
    let mut nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma", "Delta"]).await;
    let delta = nodes.pop().unwrap();
    let (platform, dir) = (delta.platform.clone(), delta.dir);
    delta.engine.shutdown().await;
    let echo = lab.start("Echo").await;
    admit(&nodes[0], &echo).await;
    let top = epoch_of(&nodes[0]);
    // While offline, Delta edited the desk itself: a higher epoch, the
    // members it knew.
    let path = dir.path().join(FILE);
    let mut saved = LocalState::load(&path);
    let desk = saved.desk.as_mut().unwrap();
    desk.state.epoch = top + 3;
    saved.save(&path).unwrap();
    let delta = lab.boot("Delta", platform, dir).await;
    nodes.push(echo);
    nodes.push(delta);
    settle().await;
    settle().await;
    for node in &nodes {
        assert_eq!(members_of(node).len(), 5, "{}", node.engine.computer_id());
    }
    let epochs: Vec<_> = nodes.iter().map(epoch_of).collect();
    assert!(epochs.iter().all(|e| *e == epochs[0]), "{epochs:?}");
}

/// A member that holds another desk key than its desk-mate (it was offline
/// when the key was rotated) is refused as "not a member", though the
/// desk-mate lists it: that is reported once, as a problem naming it, and
/// the report goes when the computer gets in.
#[tokio::test]
async fn a_listed_member_with_another_desk_key_is_reported_once() {
    use mouser_proto::control::ProblemKind;
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, other) = {
        let (d, a) = roles(&nodes);
        (&nodes[d], &nodes[a])
    };
    let right = other.engine.ctx.core.guard().desk_key;
    other.engine.ctx.core.guard().desk_key = Some([9; 32]);
    let id = other.engine.computer_id();
    let conn = dialler.engine.ctx.core.guard().peers[&id].conn.clone();
    conn.close(0u32.into(), b"done");
    // Whichever side checks the other's proof first closes the link: the
    // report is made there, once.
    let refused = |node: &TestNode| {
        let mut open = node.engine.ctx.problems.open();
        open.retain(|p| p.kind == ProblemKind::JoinRefused);
        open
    };
    eventually("the refusal to be reported", || async {
        !refused(dialler).is_empty() || !refused(other).is_empty()
    })
    .await;
    settle().await;
    for node in [dialler, other] {
        assert!(refused(node).len() <= 1, "{:?}", refused(node));
    }
    other.engine.ctx.core.guard().desk_key = right;
    eventually("the report to go", || async {
        refused(dialler).is_empty() && refused(other).is_empty()
    })
    .await;
}

/// The Displays screen reads `layout.get`: a computer taken off the desk is
/// gone from it at once, on the computer that removed it and on the others.
#[tokio::test]
async fn a_removed_computer_is_gone_from_every_arrangement_at_once() {
    use mouser_proto::control::{Empty, Method};
    use serde_json::json;
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
    let gone = nodes[2].engine.computer_id();
    let _: Empty = call(
        &nodes[0],
        Method::DeskRemoveMember,
        json!({ "computer_id": gone }),
    )
    .await;
    for node in &nodes[..2] {
        eventually("the arrangement without the removed computer", || async {
            let layout = layout_of(node).await;
            let shown = layout["displays"].as_array().unwrap().len();
            shown == 2 && layout["placements"].as_array().unwrap().len() == 2
        })
        .await;
    }
}
