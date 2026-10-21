use super::*;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::{DeskState, Layout};

fn desk(id: u8) -> PersistedDesk {
    PersistedDesk {
        desk_id: DeskId([id; 16]),
        name: "Home office".into(),
        state: DeskState {
            epoch: 7,
            author: ComputerId([1; 16]),
            members: Vec::new(),
            layout: Layout::default(),
            allowlist: vec!["10.0.0.0/8".into()],
        },
    }
}

#[test]
fn missing_file_is_unjoined() {
    let dir = tempfile::tempdir().unwrap();
    let state = LocalState::load(&dir.path().join(FILE));
    assert_eq!(state, LocalState::default());
    assert_eq!(state.membership, Membership::Unjoined);
    assert_eq!(state.desk, None);
}

#[test]
fn every_membership_round_trips_with_layout_and_epoch() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    for membership in [
        Membership::Unjoined,
        Membership::Pending(DeskId([2; 16])),
        Membership::Member(DeskId([3; 16])),
    ] {
        let state = LocalState {
            membership,
            desk: Some(desk(3)),
            ..LocalState::default()
        };
        state.save(&path);
        let loaded = LocalState::load(&path);
        assert_eq!(loaded, state);
        assert_eq!(loaded.desk.unwrap().state.epoch, 7);
    }
}

#[test]
fn the_file_holds_no_key_material() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    LocalState {
        membership: Membership::Member(DeskId([3; 16])),
        desk: Some(desk(3)),
        ..LocalState::default()
    }
    .save(&path);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("key"), "{text}");
    assert_eq!(
        desk_key_name(&DeskId([3; 16])),
        format!("desk_key/{}", "03".repeat(16))
    );
}

#[test]
#[should_panic(expected = "cannot read")]
fn unreadable_state_is_fatal() {
    let dir = tempfile::tempdir().unwrap();
    LocalState::load(dir.path());
}

#[test]
fn removed_displays_survive_a_restart_and_default_to_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(FILE);
    let state = LocalState {
        removed: std::collections::BTreeSet::from(["b2".to_string()]),
        ..LocalState::default()
    };
    state.save(&path);
    assert_eq!(LocalState::load(&path), state);
    // A file from before displays could be removed has none.
    std::fs::write(
        &path,
        r#"{"membership":"Unjoined","desk":null,"displays":{}}"#,
    )
    .unwrap();
    assert!(LocalState::load(&path).removed.is_empty());
}
