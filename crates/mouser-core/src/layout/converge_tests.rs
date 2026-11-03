//! T545: two members whose reports of a display disagree settle instead of
//! publishing at each other forever.

use crate::lock::LockExt;
use crate::testing::*;

#[tokio::test]
async fn members_that_disagree_about_a_display_stop_publishing_within_a_few_hops() {
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    b.platform.set_displays(vec![
        screen("dock-1", (0.0, 0.0)),
        screen("dock-2", (1920.0, 0.0)),
    ]);
    for node in [a, b] {
        eventually("both docked displays to be placed", || async {
            node.engine.layout().placements.len() == 3
        })
        .await;
    }
    settle().await;
    assert_eq!(epoch_of(a), epoch_of(b));
    let docked = epoch_of(a);
    // Alpha's report of Beta's second display is not what Beta reports.
    {
        let mut core = a.engine.ctx.core.guard();
        let beta = b.engine.computer_id();
        let displays = core.state.displays.get_mut(&beta).unwrap();
        displays[1].origin_local = (3000, 0);
    }
    a.engine.ctx.on_local_displays();
    for _ in 0..4 {
        settle().await;
    }
    let settled = epoch_of(a);
    assert!(
        settled <= docked + 3,
        "the epoch rose from {docked} to {settled}"
    );
    for _ in 0..4 {
        settle().await;
    }
    assert_eq!(epoch_of(a), settled);
    assert_eq!(epoch_of(b), settled);
}
