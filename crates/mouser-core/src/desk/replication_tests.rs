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
