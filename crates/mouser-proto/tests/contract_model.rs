use mouser_proto::ids::{ComputerId, DevicePublicKey};
use mouser_proto::model::*;

fn did(key: &str) -> DisplayId {
    DisplayId {
        computer: ComputerId([1; 16]),
        key: key.into(),
    }
}

fn display() -> Display {
    Display {
        display_id: did("edid-1"),
        name: "Studio".into(),
        pixel_size: (5120, 2880),
        logical_size: (2560.0, 1440.0),
        scale: 2.0,
        physical_size_mm: Some((597.0, 336.0)),
        origin_local: (-2560, 0),
        included: true,
    }
}

fn layout() -> Layout {
    Layout {
        placements: vec![Placement {
            display_id: did("edid-1"),
            x: 10,
            y: -20,
        }],
        edge_options: vec![EdgeOptions::new(did("edid-1"), Side::Right)],
        ..Layout::default()
    }
}

fn member() -> Member {
    Member {
        computer_id: ComputerId([2; 16]),
        device_public_key: DevicePublicKey([3; 32]),
        display_name: "Desk Mac".into(),
        os: Os::Macos,
        mouser_version: "0.1.0".into(),
        joined_at: "2026-01-01T00:00:00Z".into(),
    }
}

fn roundtrip<T>(v: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(v).unwrap();
    assert_eq!(&serde_json::from_str::<T>(&json).unwrap(), v);
    let bytes = postcard::to_stdvec(v).unwrap();
    assert_eq!(&postcard::from_bytes::<T>(&bytes).unwrap(), v);
}

#[test]
fn display_roundtrips_with_and_without_physical_size() {
    let mut d = display();
    roundtrip(&d);
    d.physical_size_mm = None;
    roundtrip(&d);
}

#[test]
fn layout_defaults_and_roundtrip() {
    let d = Layout::default();
    assert!(d.placements.is_empty() && d.edge_options.is_empty());
    assert!(!d.equalize_dimensions);
    assert!(d.block_fullscreen);
    assert!(EdgeOptions::new(did("a"), Side::Top).block_corners);
    roundtrip(&layout());
}

#[test]
fn member_and_desk_state_roundtrip() {
    roundtrip(&member());
    let s = DeskState {
        epoch: u64::MAX,
        author: ComputerId([5; 16]),
        members: vec![member()],
        layout: layout(),
        allowlist: vec!["192.168.0.0/16".into()],
    };
    roundtrip(&s);
}

#[test]
fn enums_serialize_as_contract_names() {
    assert_eq!(serde_json::to_string(&Os::Linux).unwrap(), "\"linux\"");
    assert_eq!(serde_json::to_string(&Side::Bottom).unwrap(), "\"bottom\"");
    assert_eq!(
        serde_json::to_string(&PermissionKind::InputMonitoring).unwrap(),
        "\"input_monitoring\""
    );
    assert_eq!(
        serde_json::to_string(&PermissionState::NotApplicable).unwrap(),
        "\"not_applicable\""
    );
}

fn state(epoch: u64, author: u8) -> DeskState {
    DeskState {
        epoch,
        author: ComputerId([author; 16]),
        members: vec![],
        layout: Layout::default(),
        allowlist: vec![],
    }
}

#[test]
fn supersedes_orders_by_epoch_then_author() {
    assert!(state(2, 1).supersedes(&state(1, 9)));
    assert!(!state(1, 9).supersedes(&state(2, 1)));
    assert!(state(5, 2).supersedes(&state(5, 1)));
    assert!(!state(5, 1).supersedes(&state(5, 2)));
    assert!(!state(5, 1).supersedes(&state(5, 1)));
}
