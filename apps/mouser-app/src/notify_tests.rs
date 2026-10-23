use std::time::{Duration, Instant};

use mouser_proto::control::{Notification, Problem, ProblemKind, Topic};
use serde_json::json;

use crate::notify::{LOST_HOLD, Notice, QUIET_FOR, Toasts, gated};
use crate::support_tests::FakeNotifier;

pub(crate) fn problem_note(cleared: bool) -> Notification {
    note_of(
        "missing_permission:accessibility",
        ProblemKind::MissingPermission,
        "Mouser needs Accessibility access.",
        cleared,
    )
}

fn note_of(id: &str, kind: ProblemKind, message: &str, cleared: bool) -> Notification {
    let problem = Problem {
        problem_id: id.into(),
        kind,
        computer: "00".repeat(16).parse().unwrap(),
        message: message.into(),
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

fn attention(body: &str) -> Option<Notice> {
    Some(Notice {
        title: "Mouser needs your attention".into(),
        body: body.into(),
    })
}

#[test]
fn a_raised_problem_becomes_a_system_notification() {
    let mut toasts = Toasts::default();
    assert_eq!(
        toasts.notice(&problem_note(false), Instant::now()),
        attention("Mouser needs Accessibility access.")
    );
}

#[test]
fn a_cleared_problem_and_other_topics_make_no_notification() {
    let mut toasts = Toasts::default();
    let now = Instant::now();
    assert_eq!(toasts.notice(&problem_note(true), now), None);
    let status = Notification {
        jsonrpc: "2.0".into(),
        method: Topic::Status,
        params: json!({}),
    };
    assert_eq!(toasts.notice(&status, now), None);
}

#[test]
fn only_new_problems_reach_the_notifier() {
    let notifier = FakeNotifier::default();
    let mut toasts = Toasts::default();
    let now = Instant::now();
    toasts.bridge(&problem_note(false), now, &notifier);
    toasts.bridge(&problem_note(true), now, &notifier);
    assert_eq!(
        *notifier.shown.lock().unwrap(),
        vec![(
            "Mouser needs your attention".to_string(),
            "Mouser needs Accessibility access.".to_string()
        )]
    );
}

#[test]
fn a_blocked_crossing_never_becomes_a_system_notification() {
    let mut toasts = Toasts::default();
    let blocked = note_of(
        "crossing_blocked:display:left",
        ProblemKind::CrossingBlocked,
        "The pointer is in a screen corner, and corners do not cross.",
        false,
    );
    assert_eq!(toasts.notice(&blocked, Instant::now()), None);
}

#[test]
fn a_hold_the_watchdog_took_back_never_becomes_a_system_notification() {
    let mut toasts = Toasts::default();
    let lost = note_of(
        "platform_limitation:platform_problem:0123456789abcdef",
        ProblemKind::PlatformLimitation,
        LOST_HOLD,
        false,
    );
    assert_eq!(toasts.notice(&lost, Instant::now()), None);
    // Other problems the platform notices (e.g. a firewall block) still do.
    let firewall = note_of(
        "platform_limitation:platform_problem:fedcba9876543210",
        ProblemKind::PlatformLimitation,
        "Windows Firewall is blocking Mouser.",
        false,
    );
    assert_eq!(
        toasts.notice(&firewall, Instant::now()),
        attention("Windows Firewall is blocking Mouser.")
    );
}

#[test]
fn the_hold_text_is_the_one_the_windows_watchdog_raises() {
    assert!(LOST_HOLD.starts_with("Mouser lost its hold on the keyboard and mouse"));
    #[cfg(target_os = "windows")]
    assert_eq!(LOST_HOLD, mouser_platform_windows::LOST_HOLD);
}

#[test]
fn a_problem_raised_twice_without_clearing_shows_once() {
    let mut toasts = Toasts::default();
    let now = Instant::now();
    assert!(toasts.notice(&problem_note(false), now).is_some());
    assert_eq!(
        toasts.notice(&problem_note(false), now + QUIET_FOR * 2),
        None
    );
}

#[test]
fn a_problem_that_comes_back_stays_quiet_for_a_while() {
    let mut toasts = Toasts::default();
    let start = Instant::now();
    assert!(toasts.notice(&problem_note(false), start).is_some());
    // Cleared and raised again, a minute apart, for most of the quiet spell.
    let minute = Duration::from_secs(60);
    for i in 1..10 {
        let at = start + minute * i;
        assert_eq!(toasts.notice(&problem_note(true), at), None);
        assert_eq!(toasts.notice(&problem_note(false), at), None, "minute {i}");
    }
    assert_eq!(toasts.notice(&problem_note(true), start + QUIET_FOR), None);
    // Once the quiet spell has passed it may be shown again.
    assert_eq!(
        toasts.notice(&problem_note(false), start + QUIET_FOR),
        attention("Mouser needs Accessibility access.")
    );
}

#[test]
fn the_quiet_spell_is_per_problem() {
    let mut toasts = Toasts::default();
    let now = Instant::now();
    assert!(toasts.notice(&problem_note(false), now).is_some());
    let refused = note_of(
        "join_refused:kitchen",
        ProblemKind::JoinRefused,
        "Kitchen refused to join.",
        false,
    );
    assert_eq!(
        toasts.notice(&refused, now),
        attention("Kitchen refused to join.")
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
