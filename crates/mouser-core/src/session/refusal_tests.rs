//! A computer that refuses us as "not a member of this desk" is not dialled
//! on every turn (T544).

use std::time::Duration;

use super::refusal::{Refusals, wait_after};
use super::{DIAL_ATTEMPT, liveness::OFFLINE_GRACE};
use crate::lock::LockExt;
use crate::testing::*;
use mouser_proto::ids::ComputerId;

#[test]
fn the_wait_doubles_with_each_refusal_up_to_the_longest_reconnect_wait() {
    assert_eq!(wait_after(1), DIAL_ATTEMPT);
    assert_eq!(wait_after(2), DIAL_ATTEMPT * 2);
    assert_eq!(wait_after(3), DIAL_ATTEMPT * 4);
    assert_eq!(wait_after(4), OFFLINE_GRACE);
    assert_eq!(wait_after(200), OFFLINE_GRACE);
}

#[test]
fn a_refused_computer_waits_and_a_success_forgets_the_refusals() {
    let mut refusals = Refusals::default();
    let peer = ComputerId([1; 16]);
    let t0 = std::time::Instant::now();
    assert!(!refusals.waiting(peer, t0));
    refusals.refused(peer, t0);
    assert!(refusals.waiting(peer, t0 + DIAL_ATTEMPT - Duration::from_millis(1)));
    assert!(!refusals.waiting(peer, t0 + DIAL_ATTEMPT));
    refusals.refused(peer, t0);
    assert!(refusals.waiting(peer, t0 + DIAL_ATTEMPT * 2 - Duration::from_millis(1)));
    refusals.accepted(peer);
    assert!(!refusals.waiting(peer, t0));
}

#[tokio::test]
async fn a_peer_that_refuses_as_not_a_member_is_dialled_a_handful_of_times_in_a_second() {
    let lab = Lab::new();
    let nodes = logged_desk_of(&lab, &["Alpha", "Beta"]).await;
    let (dialler, other) = {
        let (d, a) = roles(&nodes);
        (&nodes[d], &nodes[a])
    };
    // The other computer holds another desk key: it refuses our proof.
    other.engine.ctx.core.guard().desk_key = Some([9; 32]);
    let id = other.engine.computer_id();
    let conn = dialler.engine.ctx.core.guard().peers[&id].conn.clone();
    conn.close(0u32.into(), b"done");
    tokio::time::sleep(Duration::from_secs(1)).await;
    let refused = log_events(dialler, "peer.disconnected")
        .iter()
        .filter(|r| r["fields"]["reason"] == "removed" || r["reason"] == "removed")
        .count();
    assert!((1..=2).contains(&refused), "{refused} refusals in a second");
}
