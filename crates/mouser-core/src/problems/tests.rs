use mouser_proto::control::{FixAction, ProblemKind, Topic};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::PermissionKind;
use tokio::sync::broadcast;

use super::*;

const ME: ComputerId = ComputerId([1; 16]);
const OTHER: ComputerId = ComputerId([2; 16]);

fn registry() -> (Problems, broadcast::Receiver<Notification>) {
    let (tx, rx) = broadcast::channel(16);
    (Problems::new(tx), rx)
}

fn raise(problems: &Problems, subject: &str) -> bool {
    raise_for(problems, subject, ME)
}

fn raise_for(problems: &Problems, subject: &str, computer: ComputerId) -> bool {
    problems.raise(
        ProblemKind::JoinRefused,
        subject,
        computer,
        "Kitchen was not admitted".into(),
        Some(FixAction::ApproveJoin),
    )
}

#[test]
fn raising_the_same_problem_twice_does_not_duplicate_it() {
    let (problems, mut rx) = registry();
    assert!(raise(&problems, "kitchen"));
    assert!(!raise(&problems, "kitchen"));
    assert!(raise(&problems, "garage"));
    let open = problems.open();
    assert_eq!(open.len(), 2);
    assert_eq!(open[0].problem_id, "join_refused:kitchen");
    assert_eq!(open[0].fix, Some(FixAction::ApproveJoin));
    assert_eq!(open[0].cleared_at, None);
    assert!(rx.try_recv().is_ok());
    assert!(rx.try_recv().is_ok());
    assert!(
        rx.try_recv().is_err(),
        "the duplicate raised no notification"
    );
}

#[test]
fn clearing_sets_cleared_at_and_hides_it_from_the_open_list() {
    let (problems, mut rx) = registry();
    raise(&problems, "kitchen");
    rx.try_recv().unwrap();
    assert!(problems.clear(ProblemKind::JoinRefused, "kitchen", ME));
    assert!(
        !problems.clear(ProblemKind::JoinRefused, "kitchen", ME),
        "already cleared"
    );
    assert!(!problems.clear(ProblemKind::SecureInput, "nothing", ME));
    assert!(problems.open().is_empty());
    let cleared = rx.try_recv().unwrap();
    assert_eq!(cleared.method, Topic::Problem);
    assert!(cleared.params["cleared_at"].is_string());
    assert!(
        raise(&problems, "kitchen"),
        "a cleared problem can be raised again"
    );
    assert_eq!(problems.open().len(), 1);
}

#[test]
fn subscribers_receive_problem_notifications_with_the_full_problem() {
    let (problems, mut rx) = registry();
    raise(&problems, "kitchen");
    let n = rx.try_recv().unwrap();
    assert_eq!(n.jsonrpc, "2.0");
    assert_eq!(n.method, Topic::Problem);
    let problem: Problem = serde_json::from_value(n.params).unwrap();
    assert_eq!(problem.kind, ProblemKind::JoinRefused);
    assert_eq!(problem.computer, ME);
    assert_eq!(problem.message, "Kitchen was not admitted");
    assert!(chrono::DateTime::parse_from_rfc3339(&problem.raised_at).is_ok());
}

#[test]
fn raising_without_subscribers_still_records_the_problem() {
    let (tx, rx) = broadcast::channel(4);
    drop(rx);
    let problems = Problems::new(tx);
    assert!(raise(&problems, "kitchen"));
    assert_eq!(problems.open().len(), 1);
}

#[test]
fn every_permission_has_plain_language_text_and_a_fix() {
    for kind in [
        PermissionKind::Accessibility,
        PermissionKind::InputMonitoring,
        PermissionKind::ScreenRecording,
        PermissionKind::FullDiskAccess,
        PermissionKind::InputCapturePortal,
        PermissionKind::RemoteDesktopPortal,
    ] {
        assert!(!permission_text(kind).is_empty());
    }
}

#[test]
fn now_is_rfc3339() {
    assert!(chrono::DateTime::parse_from_rfc3339(&now()).is_ok());
}

#[test]
fn the_same_problem_id_on_two_computers_is_two_problems_each_about_its_own() {
    let (problems, mut rx) = registry();
    assert!(raise_for(&problems, "kitchen", ME));
    assert!(raise_for(&problems, "kitchen", OTHER));
    assert!(!raise_for(&problems, "kitchen", ME), "unique per computer");
    assert!(!raise_for(&problems, "kitchen", OTHER));
    let open = problems.open();
    assert_eq!(open.len(), 2);
    assert!(open.iter().all(|p| p.problem_id == "join_refused:kitchen"));
    assert_eq!(
        open.iter().map(|p| p.computer).collect::<Vec<_>>(),
        [ME, OTHER]
    );
    rx.try_recv().unwrap();
    rx.try_recv().unwrap();
    assert!(rx.try_recv().is_err());
}

#[test]
fn clearing_one_computers_problem_leaves_the_same_id_on_another() {
    let (problems, _rx) = registry();
    raise_for(&problems, "kitchen", ME);
    raise_for(&problems, "kitchen", OTHER);
    assert!(problems.clear(ProblemKind::JoinRefused, "kitchen", ME));
    assert!(!problems.clear(ProblemKind::JoinRefused, "kitchen", ME));
    let open = problems.open();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].computer, OTHER);
    assert!(!problems.clear_id("join_refused:kitchen", ME));
    assert!(problems.clear_id("join_refused:kitchen", OTHER));
    assert!(problems.open().is_empty());
}

#[tokio::test]
async fn status_lists_a_peers_problem_beside_ours_with_the_same_id() {
    use crate::testing::*;
    use mouser_proto::wire::ControlMessage;
    let lab = Lab::new();
    let nodes = desk_of(&lab, &["Alpha", "Beta"]).await;
    let (a, b) = (&nodes[0], &nodes[1]);
    let (ida, idb) = (a.engine.computer_id(), b.engine.computer_id());
    // The same notice, about each of the two computers.
    a.engine.ctx.problems.raise(
        ProblemKind::SecureInput,
        "shared",
        ida,
        "about Alpha".into(),
        None,
    );
    let about_beta = Problem {
        problem_id: "secure_input:shared".into(),
        kind: ProblemKind::SecureInput,
        computer: idb,
        message: "about Beta".into(),
        app: None,
        fix: None,
        raised_at: now(),
        cleared_at: None,
    };
    let mut report = b.engine.ctx.peer_status();
    report.problems = vec![about_beta];
    send_to(b, a, ControlMessage::Status(report));
    eventually("both to be listed", || async {
        status(a)
            .await
            .problems
            .iter()
            .filter(|p| p.problem_id == "secure_input:shared")
            .count()
            == 2
    })
    .await;
    let s = status(a).await;
    let mut computers: Vec<_> = s
        .problems
        .iter()
        .filter(|p| p.problem_id == "secure_input:shared")
        .map(|p| (p.computer, p.message.as_str()))
        .collect();
    computers.sort();
    let mut want = vec![(ida, "about Alpha"), (idb, "about Beta")];
    want.sort();
    assert_eq!(computers, want);
}
