use mouser_proto::control::{Notification, Problem, ProblemKind, Topic};
use serde_json::json;

use crate::notify::{Notice, bridge, gated, notice};
use crate::support_tests::FakeNotifier;

pub(crate) fn problem_note(cleared: bool) -> Notification {
    let problem = Problem {
        problem_id: "missing_permission:accessibility".into(),
        kind: ProblemKind::MissingPermission,
        computer: "00".repeat(16).parse().unwrap(),
        message: "Mouser needs Accessibility access.".into(),
        fix: None,
        raised_at: "2026-10-21T00:00:00Z".into(),
        cleared_at: cleared.then(|| "2026-10-21T00:01:00Z".into()),
    };
    Notification {
        jsonrpc: "2.0".into(),
        method: Topic::Problem,
        params: serde_json::to_value(problem).unwrap(),
    }
}

#[test]
fn a_raised_problem_becomes_a_system_notification() {
    assert_eq!(
        notice(&problem_note(false)),
        Some(Notice {
            title: "Mouser needs your attention".into(),
            body: "Mouser needs Accessibility access.".into(),
        })
    );
}

#[test]
fn a_cleared_problem_and_other_topics_make_no_notification() {
    assert_eq!(notice(&problem_note(true)), None);
    let status = Notification {
        jsonrpc: "2.0".into(),
        method: Topic::Status,
        params: json!({}),
    };
    assert_eq!(notice(&status), None);
}

#[test]
fn only_new_problems_reach_the_notifier() {
    let notifier = FakeNotifier::default();
    bridge(&problem_note(false), &notifier);
    bridge(&problem_note(true), &notifier);
    assert_eq!(
        *notifier.shown.lock().unwrap(),
        vec![(
            "Mouser needs your attention".to_string(),
            "Mouser needs Accessibility access.".to_string()
        )]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn turning_notifications_off_silences_the_notifier_live() {
    let f = crate::support_tests::fixture().await;
    let control = f.host.control();
    let notifier = FakeNotifier::default();
    gated(&control, &notifier).await.show("on", "1");
    control
        .call(
            mouser_proto::control::Method::ConfigSet,
            Some(json!({"key": "general.notifications", "value": false})),
        )
        .await
        .unwrap();
    gated(&control, &notifier).await.show("off", "2");
    control
        .call(
            mouser_proto::control::Method::ConfigSet,
            Some(json!({"key": "general.notifications", "value": true})),
        )
        .await
        .unwrap();
    gated(&control, &notifier).await.show("on again", "3");
    let shown = notifier.shown.lock().unwrap();
    assert_eq!(
        *shown,
        vec![
            ("on".to_string(), "1".to_string()),
            ("on again".to_string(), "3".to_string())
        ]
    );
}
