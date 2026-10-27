use mouser_proto::ids::ComputerId;
use mouser_proto::ids::DevicePublicKey;
use mouser_proto::model::{DeskState, Layout, Member, Os};

use super::replication::adopt;

fn state(epoch: u64, author: u8, members: &[u8]) -> DeskState {
    DeskState {
        epoch,
        author: ComputerId([author; 16]),
        members: members
            .iter()
            .map(|n| Member {
                computer_id: ComputerId([*n; 16]),
                device_public_key: DevicePublicKey([*n; 32]),
                display_name: format!("Computer {n}"),
                os: Os::Linux,
                mouser_version: "0.1.0".into(),
                joined_at: "2026-10-21T00:00:00.000Z".into(),
            })
            .collect(),
        layout: Layout::default(),
        allowlist: Vec::new(),
    }
}

#[test]
fn a_higher_epoch_wins() {
    let mut mine = state(3, 1, &[1, 2]);
    assert!(adopt(&mut mine, state(4, 1, &[1, 2, 3])));
    assert_eq!(mine.members.len(), 3);
    assert!(!adopt(&mut mine, state(3, 9, &[1])));
    assert_eq!(mine.epoch, 4);
}

#[test]
fn at_the_same_epoch_the_higher_computer_id_wins() {
    let mut mine = state(5, 2, &[1, 2, 3]);
    assert!(adopt(&mut mine, state(5, 7, &[1, 2, 4])));
    assert_eq!(mine.author, ComputerId([7; 16]));
    assert!(!adopt(&mut mine, state(5, 2, &[1, 2, 3])));
    assert_eq!(mine.members[2].computer_id, ComputerId([4; 16]));
}

#[test]
fn an_identical_state_is_not_adopted_again() {
    let mut mine = state(5, 2, &[1, 2]);
    assert!(!adopt(&mut mine, state(5, 2, &[1, 2])));
}

#[test]
fn every_member_converges_on_the_same_list_whatever_the_delivery_order() {
    let offers = [
        state(2, 1, &[1, 2]),
        state(3, 2, &[1, 2, 3]),
        state(3, 3, &[1, 2, 4]),
        state(1, 9, &[9]),
    ];
    let orders: [[usize; 4]; 3] = [[0, 1, 2, 3], [3, 2, 1, 0], [1, 3, 0, 2]];
    let mut results = Vec::new();
    for order in orders {
        let mut mine = state(1, 1, &[1]);
        for i in order {
            adopt(&mut mine, offers[i].clone());
        }
        results.push(mine);
    }
    assert!(results.windows(2).all(|w| w[0] == w[1]));
    assert_eq!(results[0].author, ComputerId([3; 16]));
}

mod renames {
    use mouser_proto::control::{Empty, Method, Topic};
    use serde_json::json;

    use crate::testing::*;

    async fn name_of(viewer: &TestNode, who: mouser_proto::ids::ComputerId) -> String {
        status(viewer)
            .await
            .computers
            .into_iter()
            .find(|c| c.id == who)
            .expect("listed")
            .name
    }

    async fn configured_name(node: &TestNode) -> String {
        let name: String = call(
            node,
            Method::ConfigGet,
            json!({"key": "general.computer_name"}),
        )
        .await;
        name
    }

    #[tokio::test]
    async fn setting_the_computer_name_renames_this_computer_desk_wide_and_empty_means_the_os_name()
    {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let alpha = nodes[0].engine.computer_id();
        let before = epoch_of(&nodes[1]);
        let _: Empty = call(
            &nodes[0],
            Method::ConfigSet,
            json!({"key": "general.computer_name", "value": "Desk Mac"}),
        )
        .await;
        eventually("the new name on the other computer", || async {
            name_of(&nodes[1], alpha).await == "Desk Mac"
        })
        .await;
        assert_eq!(name_of(&nodes[0], alpha).await, "Desk Mac");
        assert_eq!(epoch_of(&nodes[1]), before + 1);
        // Empty: back to the name the operating system gives.
        let _: Empty = call(
            &nodes[0],
            Method::ConfigSet,
            json!({"key": "general.computer_name", "value": ""}),
        )
        .await;
        eventually("the OS name on the other computer", || async {
            name_of(&nodes[1], alpha).await == "Alpha"
        })
        .await;
        assert_eq!(name_of(&nodes[0], alpha).await, "Alpha");
    }

    #[tokio::test]
    async fn a_computer_renamed_from_another_writes_the_name_into_its_own_settings() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let beta = nodes[1].engine.computer_id();
        let alpha = nodes[0].engine.computer_id();
        let mut notes = nodes[1].engine.control().notifications();
        let _: Empty = call(
            &nodes[0],
            Method::DeskRename,
            json!({"computer": beta, "name": "Studio PC"}),
        )
        .await;
        let change = loop {
            let change = recv_topic(&mut notes, Topic::Config).await;
            if change["key"] == "general.computer_name" {
                break change;
            }
        };
        assert_eq!(change["new"], "Studio PC");
        assert_eq!(change["origin"], "desk_mate");
        assert_eq!(change["from"], alpha.to_string());
        assert_eq!(configured_name(&nodes[1]).await, "Studio PC");
        // The renamed computer introduces itself by the new name.
        assert_eq!(name_of(&nodes[1], beta).await, "Studio PC");
        // And the renamer's own settings are untouched.
        assert_eq!(configured_name(&nodes[0]).await, "");
    }

    #[tokio::test]
    async fn a_computer_offline_during_a_rename_adopts_it_on_reconnect() {
        let lab = Lab::new();
        let mut nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
        let gamma = nodes.pop().unwrap();
        let gamma_id = gamma.engine.computer_id();
        let beta = nodes[1].engine.computer_id();
        let (platform, dir) = (gamma.platform.clone(), gamma.dir);
        gamma.engine.shutdown().await;
        let _: Empty = call(
            &nodes[0],
            Method::DeskRename,
            json!({"computer": beta, "name": "Studio PC"}),
        )
        .await;
        let _: Empty = call(
            &nodes[0],
            Method::DeskRename,
            json!({"computer": gamma_id, "name": "Gamma Renamed"}),
        )
        .await;
        let gamma = lab.boot("Gamma", platform, dir).await;
        eventually("Gamma to hear of both renames", || async {
            name_of(&gamma, beta).await == "Studio PC"
                && name_of(&gamma, gamma_id).await == "Gamma Renamed"
        })
        .await;
        assert_eq!(configured_name(&gamma).await, "Gamma Renamed");
    }
}
