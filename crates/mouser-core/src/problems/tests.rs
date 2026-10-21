use mouser_proto::control::{FixAction, ProblemKind, Topic};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::PermissionKind;
use tokio::sync::broadcast;

use super::*;

const ME: ComputerId = ComputerId([1; 16]);

fn registry() -> (Problems, broadcast::Receiver<Notification>) {
    let (tx, rx) = broadcast::channel(16);
    (Problems::new(tx), rx)
}

fn raise(problems: &Problems, subject: &str) -> bool {
    problems.raise(
        ProblemKind::JoinRefused,
        subject,
        ME,
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
    assert!(problems.clear(ProblemKind::JoinRefused, "kitchen"));
    assert!(
        !problems.clear(ProblemKind::JoinRefused, "kitchen"),
        "already cleared"
    );
    assert!(!problems.clear(ProblemKind::SecureInput, "nothing"));
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
