use mouser_proto::control::{Desk, Method};
use serde_json::json;

use super::create::validate_name;
use crate::state::{LocalState, Membership, desk_key_name};
use crate::testing::{Lab, call, call_err};

#[test]
fn the_name_is_trimmed_and_must_have_1_to_64_characters() {
    assert_eq!(validate_name("  Home office \n").unwrap(), "Home office");
    assert_eq!(validate_name(&"é".repeat(64)).unwrap().chars().count(), 64);
    assert!(validate_name("").unwrap_err().contains("not 0"));
    assert!(validate_name("   ").is_err());
    assert!(
        validate_name(&"x".repeat(65))
            .unwrap_err()
            .contains("not 65")
    );
}

#[tokio::test]
async fn creating_a_desk_makes_this_computer_its_only_member_and_advertises_it() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let desk: Desk = call(&node, Method::DeskCreate, json!({"name": " Home office "})).await;
    assert_eq!(desk.name, "Home office");
    assert_eq!(desk.state.epoch, 1);
    assert_eq!(desk.state.members.len(), 1);
    assert_eq!(desk.state.members[0].computer_id, node.engine.computer_id());
    assert_eq!(desk.state.author, node.engine.computer_id());

    // 32-byte key in the secret store, never in state.json.
    use mouser_platform::Platform;
    let key = node
        .platform
        .secrets()
        .get(&desk_key_name(&desk.desk_id))
        .unwrap()
        .unwrap();
    assert_eq!(key.len(), 32);
    let saved = LocalState::load(&node.dir.path().join(crate::state::FILE));
    assert_eq!(saved.membership, Membership::Member(desk.desk_id));

    // Offered to the rest of the network.
    let advert = lab.registry.advert_of(node.engine.computer_id()).unwrap();
    assert_eq!(advert.desk, Some((desk.desk_id, "Home office".to_string())));
    assert!(node.platform.is_autostart_enabled());
}

#[tokio::test]
async fn every_desk_gets_its_own_id_and_key() {
    let lab = Lab::new();
    let (a, b) = (lab.start("Alpha").await, lab.start("Beta").await);
    let da: Desk = call(&a, Method::DeskCreate, json!({"name": "Same"})).await;
    let db: Desk = call(&b, Method::DeskCreate, json!({"name": "Same"})).await;
    assert_ne!(da.desk_id, db.desk_id);
    use mouser_platform::Platform;
    let ka = a
        .platform
        .secrets()
        .get(&desk_key_name(&da.desk_id))
        .unwrap();
    let kb = b
        .platform
        .secrets()
        .get(&desk_key_name(&db.desk_id))
        .unwrap();
    assert_ne!(ka, kb);
}

#[tokio::test]
async fn invalid_names_and_a_second_desk_are_refused() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let err = call_err(&node, Method::DeskCreate, json!({"name": "   "})).await;
    assert_eq!(err.code, -32602);
    assert!(err.message.contains("1 to 64"), "{}", err.message);
    let _: Desk = call(&node, Method::DeskCreate, json!({"name": "First"})).await;
    let err = call_err(&node, Method::DeskCreate, json!({"name": "Second"})).await;
    assert!(err.message.contains("already"), "{}", err.message);
}
