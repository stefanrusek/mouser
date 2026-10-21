use super::displays::*;
use mouser_platform::LocalDisplay;
use mouser_proto::ids::ComputerId;

#[test]
fn local_displays_map_to_wire_displays() {
    let local = LocalDisplay {
        key: "d1".into(),
        name: "Studio Display".into(),
        pixel_size: (5120, 2880),
        logical_size: (2560.0, 1440.0),
        scale: 2.0,
        physical_size_mm: Some((597.0, 336.0)),
        origin: (-2560.0, 10.0),
    };
    let wire = to_wire(
        ComputerId([4; 16]),
        std::slice::from_ref(&local),
        &Default::default(),
    );
    assert_eq!(wire.len(), 1);
    let d = &wire[0];
    assert_eq!(d.display_id.computer, ComputerId([4; 16]));
    assert_eq!(d.display_id.key, "d1");
    assert_eq!(d.pixel_size, (5120, 2880));
    assert_eq!(d.logical_size, (2560.0, 1440.0));
    assert_eq!(d.scale, 2.0);
    assert_eq!(d.physical_size_mm, Some((597.0, 336.0)));
    assert_eq!(d.origin_local, (-2560, 10));
    assert!(d.included);
    let removed = std::collections::BTreeSet::from(["d1".to_string()]);
    assert!(!to_wire(ComputerId([4; 16]), &[local], &removed)[0].included);
}

mod on_the_wire {
    use mouser_proto::control::Method;
    use mouser_proto::model::DisplayId;
    use serde_json::json;

    use crate::EngineEvent;
    use crate::testing::*;

    fn keys_placed(node: &TestNode, computer: &TestNode) -> Vec<String> {
        let mut keys: Vec<_> = node
            .engine
            .layout()
            .placements
            .into_iter()
            .filter(|p| p.display_id.computer == computer.engine.computer_id())
            .map(|p| p.display_id.key)
            .collect();
        keys.sort();
        keys
    }

    #[tokio::test]
    async fn members_exchange_displays_on_connect() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let known = a.engine.ctx.core.lock().unwrap().state.displays.clone();
        assert_eq!(
            known[&b.engine.computer_id()][0].display_id.key,
            "virtual-0"
        );
        assert_eq!(known[&a.engine.computer_id()].len(), 1);
        assert_eq!(keys_placed(a, b), ["virtual-0"]);
        assert_eq!(a.engine.layout(), b.engine.layout());
    }

    #[tokio::test]
    async fn changed_displays_replace_the_old_ones_and_offer_a_review() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let (a, b) = (&nodes[0], &nodes[1]);
        let mut events = a.engine.events();
        // Beta is docked: its one display becomes two others.
        b.platform.set_displays(vec![
            screen("dock-1", (0.0, 0.0)),
            screen("dock-2", (1920.0, 0.0)),
        ]);
        for node in [a, b] {
            eventually("the new displays in the layout", || async {
                keys_placed(node, b) == ["dock-1", "dock-2"]
            })
            .await;
        }
        assert_eq!(a.engine.layout(), b.engine.layout());
        let EngineEvent::ReviewOffered { computer, name } = events.recv().await.unwrap() else {
            panic!("a review is offered");
        };
        assert_eq!((computer, name.as_str()), (b.engine.computer_id(), "Beta"));
        let flagged: Vec<DisplayId> = a.engine.needs_review();
        assert_eq!(flagged.len(), 2);
        // Alpha's own displays were not touched.
        assert_eq!(keys_placed(a, a), ["virtual-0"]);
        assert_eq!(status(a).await.computers[1].displays, 2);
    }

    #[tokio::test]
    async fn displays_changed_while_stopped_replace_the_old_ones_on_rejoin() {
        let lab = Lab::new();
        let mut nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
        let b = nodes.pop().unwrap();
        let a = nodes.pop().unwrap();
        let (platform, dir) = (b.platform.clone(), b.dir);
        b.engine.shutdown().await;
        platform.set_displays(vec![screen("undocked", (0.0, 0.0))]);
        let b = lab.boot("Beta", platform, dir).await;
        for node in [&a, &b] {
            eventually("the new display in the layout", || async {
                keys_placed(node, &b) == ["undocked"]
            })
            .await;
        }
        assert_eq!(a.engine.layout(), b.engine.layout());
        // Back to a desk with one placement per display, and nothing else.
        assert_eq!(a.engine.layout().placements.len(), 2);
    }

    #[tokio::test]
    async fn a_departing_member_takes_its_displays_out_of_the_layout() {
        let lab = Lab::new();
        let nodes = desk_of(&lab, &["Alpha", "Beta", "Gamma"]).await;
        let gone = nodes[1].engine.computer_id();
        let _: mouser_proto::control::Empty = call(&nodes[1], Method::DeskLeave, json!({})).await;
        for node in [&nodes[0], &nodes[2]] {
            eventually("the layout to shrink", || async {
                node.engine
                    .layout()
                    .placements
                    .iter()
                    .all(|p| p.display_id.computer != gone)
                    && node.engine.layout().placements.len() == 2
            })
            .await;
        }
        assert!(nodes[1].engine.layout().placements.is_empty());
    }
}
