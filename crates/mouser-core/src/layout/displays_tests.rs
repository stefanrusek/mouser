use super::displays::*;
use crate::logging::events::DisplayChange;
use mouser_platform::LocalDisplay;
use mouser_proto::ids::ComputerId;
use mouser_proto::model::Display;

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

fn display(key: &str, name: &str, origin: (i32, i32), logical: (f32, f32)) -> Display {
    to_wire(
        ComputerId([4; 16]),
        &[LocalDisplay {
            key: key.into(),
            name: name.into(),
            pixel_size: (1920, 1080),
            logical_size: (f64::from(logical.0), f64::from(logical.1)),
            scale: 1.0,
            physical_size_mm: None,
            origin: (f64::from(origin.0), f64::from(origin.1)),
        }],
        &Default::default(),
    )
    .remove(0)
}

#[test]
fn a_display_added_removed_rearranged_or_resized_is_a_change_of_that_display() {
    let main = display("d1", "Main", (0, 0), (1920.0, 1080.0));
    let side = display("d2", "Side", (1920, 0), (1920.0, 1080.0));
    assert_eq!(
        changes(std::slice::from_ref(&main), std::slice::from_ref(&main)),
        []
    );
    assert_eq!(
        changes(std::slice::from_ref(&main), &[main.clone(), side.clone()]),
        [("Side".to_string(), DisplayChange::Added)]
    );
    assert_eq!(
        changes(&[main.clone(), side.clone()], std::slice::from_ref(&main)),
        [("Side".to_string(), DisplayChange::Removed)]
    );
    let moved = display("d2", "Side", (0, 1080), (1920.0, 1080.0));
    assert_eq!(
        changes(&[main.clone(), side.clone()], &[main.clone(), moved]),
        [("Side".to_string(), DisplayChange::Rearranged)]
    );
    let smaller = display("d2", "Side", (1920, 0), (1280.0, 720.0));
    assert_eq!(
        changes(&[main.clone(), side.clone()], &[main.clone(), smaller]),
        [("Side".to_string(), DisplayChange::Resized)]
    );
    // Moved and resized at once is both.
    let both = display("d2", "Side", (0, 0), (1280.0, 720.0));
    assert_eq!(
        changes(std::slice::from_ref(&side), &[both]),
        [
            ("Side".to_string(), DisplayChange::Rearranged),
            ("Side".to_string(), DisplayChange::Resized)
        ]
    );
    // Taking a display out of the layout is not a change of the display.
    let mut excluded = side.clone();
    excluded.included = false;
    assert_eq!(changes(&[side], &[excluded]), []);
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

    mod logged {
        //! `display.changed` (Story 2 scenario 7): written by the computer
        //! the display changed on and by every desk-mate told of it.

        use mouser_platform::Platform;
        use serde_json::json;

        use crate::testing::*;

        async fn desk() -> (Lab, Vec<TestNode>) {
            let lab = Lab::new();
            let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
            (lab, nodes)
        }

        async fn changed(node: &TestNode, computer: &TestNode, display: &str, how: &str) {
            let id = computer.engine.computer_id().to_string();
            let record = logged(node, "display.changed", |r| {
                r["fields"]["display"] == display && r["fields"]["change"] == how
            })
            .await;
            assert_eq!(record["level"], "info");
            assert_eq!(
                record["fields"],
                json!({"computer": id, "display": display, "change": how})
            );
        }

        #[tokio::test]
        async fn displays_known_at_the_start_are_not_changes() {
            let (_lab, nodes) = desk().await;
            for node in &nodes {
                assert!(log_events(node, "display.changed").is_empty());
            }
        }

        #[tokio::test]
        async fn a_display_added_rearranged_resized_and_removed_is_logged_on_both_computers() {
            let (_lab, nodes) = desk().await;
            let (a, b) = (&nodes[0], &nodes[1]);
            let original = b.platform.displays().unwrap();
            let with = |extra: mouser_platform::LocalDisplay| {
                let mut all = original.clone();
                all.push(extra);
                all
            };
            b.platform.set_displays(with(screen("side", (1920.0, 0.0))));
            changed(b, b, "side", "added").await;
            changed(a, b, "side", "added").await;

            b.platform
                .set_displays(with(screen("side", (1920.0, 1080.0))));
            changed(b, b, "side", "rearranged").await;
            changed(a, b, "side", "rearranged").await;

            let mut smaller = screen("side", (1920.0, 1080.0));
            smaller.logical_size = (1280.0, 720.0);
            b.platform.set_displays(with(smaller));
            changed(b, b, "side", "resized").await;
            changed(a, b, "side", "resized").await;

            b.platform.set_displays(original);
            changed(b, b, "side", "removed").await;
            changed(a, b, "side", "removed").await;
        }
    }
}

mod own_changes {
    use mouser_platform::Platform;

    use crate::lock::LockExt;
    use crate::testing::*;

    /// Another path that brings this computer's display list up to date (a
    /// desk-mate's layout arriving, say) before the watcher has told the
    /// engine about the change must not hide the change from the log.
    #[tokio::test]
    async fn a_change_noticed_by_another_path_first_is_still_logged_once() {
        let lab = Lab::new();
        let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
        let beta = &nodes[1];
        let mut displays = beta.platform.displays().unwrap();
        displays.push(screen("side", (1920.0, 0.0)));
        {
            // The engine's own lock is held, so the watcher's handler waits
            // behind the other path.
            let mut core = beta.engine.ctx.core.guard();
            beta.platform.set_displays(displays);
            beta.engine.ctx.sync_layout(&mut core);
        }
        logged(beta, "display.changed", |r| {
            r["fields"]["change"] == "added"
        })
        .await;
        settle().await;
        let added = log_events(beta, "display.changed")
            .into_iter()
            .filter(|r| r["fields"]["change"] == "added")
            .count();
        assert_eq!(added, 1);
    }
}
