use mouser_proto::model::Os;
use mouser_proto::wire::Modifiers;

use super::modifiers::*;

const C: u32 = 0x0007_0006;

#[test]
fn command_becomes_control_from_a_mac_to_windows_and_linux() {
    for to in [Os::Windows, Os::Linux] {
        assert_eq!(map_usage(true, Os::Macos, to, LEFT_META), LEFT_CTRL);
        assert_eq!(map_usage(true, Os::Macos, to, RIGHT_META), RIGHT_CTRL);
        assert_eq!(map_usage(true, Os::Macos, to, C), C);
        assert_eq!(map_usage(true, Os::Macos, to, LEFT_CTRL), LEFT_CTRL);
    }
}

#[test]
fn control_becomes_command_from_windows_and_linux_to_a_mac() {
    for from in [Os::Windows, Os::Linux] {
        assert_eq!(map_usage(true, from, Os::Macos, LEFT_CTRL), LEFT_META);
        assert_eq!(map_usage(true, from, Os::Macos, RIGHT_CTRL), RIGHT_META);
        assert_eq!(map_usage(true, from, Os::Macos, C), C);
        assert_eq!(map_usage(true, from, Os::Macos, LEFT_META), LEFT_META);
    }
}

#[test]
fn pairs_of_the_same_kind_pass_through() {
    for (from, to) in [
        (Os::Macos, Os::Macos),
        (Os::Windows, Os::Linux),
        (Os::Linux, Os::Windows),
        (Os::Windows, Os::Windows),
    ] {
        for usage in [LEFT_CTRL, LEFT_META, RIGHT_CTRL, RIGHT_META, C] {
            assert_eq!(map_usage(true, from, to, usage), usage);
        }
    }
}

#[test]
fn held_keys_give_the_platform_neutral_modifiers() {
    assert_eq!(held_modifiers(&[]), Modifiers::default());
    let all = held_modifiers(&[RIGHT_SHIFT, LEFT_CTRL, RIGHT_ALT, LEFT_META, C]);
    assert_eq!(
        all,
        Modifiers {
            shift: true,
            ctrl: true,
            alt: true,
            meta: true
        }
    );
    let some = held_modifiers(&[LEFT_SHIFT, RIGHT_CTRL, LEFT_ALT, RIGHT_META]);
    assert_eq!(some, all);
}

#[test]
fn with_command_as_control_off_no_key_is_mapped() {
    for (from, to) in [
        (Os::Macos, Os::Windows),
        (Os::Macos, Os::Linux),
        (Os::Windows, Os::Macos),
        (Os::Linux, Os::Macos),
        (Os::Macos, Os::Macos),
        (Os::Windows, Os::Linux),
    ] {
        for usage in [LEFT_CTRL, LEFT_META, RIGHT_CTRL, RIGHT_META, C] {
            assert_eq!(map_usage(false, from, to, usage), usage);
        }
    }
}

mod on_the_wire {
    use mouser_proto::control::{Desk, Empty, Method};
    use mouser_proto::model::DisplayId;
    use mouser_proto::wire::{ControlMessage, Modifiers};
    use mouser_testkit::{Injected, VirtualPlatform};
    use serde_json::json;

    use super::*;
    use crate::testing::*;

    /// A Mac and a Linux computer on one desk.
    async fn mac_and_linux(lab: &Lab) -> [TestNode; 2] {
        let mac = VirtualPlatform::new();
        mac.set_info(mouser_platform::PlatformInfo {
            os: mouser_platform::Os::MacOs,
            version: "26".into(),
            session: mouser_platform::SessionType::Native,
            limitations: Vec::new(),
        });
        let a = lab.start_with("Mac", mac, |_| {}).await;
        let b = lab.start("Linux").await;
        let _: Desk = call(&a, Method::DeskCreate, json!({"name": "Home"})).await;
        admit(&a, &b).await;
        let nodes = [a, b];
        ready(&nodes).await;
        nodes
    }

    fn key(seq: u64, usage: u32, down: bool) -> ControlMessage {
        ControlMessage::Key {
            seq,
            usage,
            down,
            modifiers: Modifiers::default(),
        }
    }

    /// The keys injected so far on `node`, as (usage, down).
    fn keys(node: &TestNode) -> Vec<(u32, bool)> {
        node.platform
            .injected()
            .into_iter()
            .filter_map(|i| match i {
                Injected::Key { usage, down } => Some((usage, down)),
                _ => None,
            })
            .collect()
    }

    async fn type_shortcut(from: &TestNode, to: &TestNode, usage: u32) {
        send_to(
            from,
            to,
            ControlMessage::Enter {
                display_id: DisplayId {
                    computer: to.engine.computer_id(),
                    key: "virtual-0".into(),
                },
                x: 1.0,
                y: 1.0,
                seq_base: 1,
                modifiers_down: Modifiers::default(),
            },
        );
        send_to(from, to, key(2, usage, true));
        send_to(from, to, key(3, C, true));
        send_to(from, to, key(4, C, false));
        send_to(from, to, key(5, usage, false));
        eventually("the shortcut to be injected", || async {
            keys(to).len() == 4
        })
        .await;
    }

    #[tokio::test]
    async fn command_is_control_on_windows_and_linux_while_the_setting_is_on() {
        let lab = Lab::new();
        let nodes = mac_and_linux(&lab).await;
        let (mac, linux) = (&nodes[0], &nodes[1]);
        type_shortcut(mac, linux, LEFT_META).await;
        assert_eq!(
            keys(linux),
            [(LEFT_CTRL, true), (C, true), (C, false), (LEFT_CTRL, false)]
        );
    }

    #[tokio::test]
    async fn command_is_the_windows_key_and_control_is_control_while_it_is_off() {
        let lab = Lab::new();
        let nodes = mac_and_linux(&lab).await;
        let (mac, linux) = (&nodes[0], &nodes[1]);
        let _: Empty = call(
            linux,
            Method::ConfigSet,
            json!({"key": "keyboard.command_as_control", "value": false}),
        )
        .await;
        type_shortcut(mac, linux, LEFT_META).await;
        assert_eq!(
            keys(linux),
            [(LEFT_META, true), (C, true), (C, false), (LEFT_META, false)]
        );
        linux.platform.take_injected();
        type_shortcut(mac, linux, LEFT_CTRL).await;
        assert_eq!(
            keys(linux),
            [(LEFT_CTRL, true), (C, true), (C, false), (LEFT_CTRL, false)]
        );
    }
}
