use std::time::{Duration, Instant};

use mouser_proto::control::{Notification, Problem, ProblemKind, Topic};
use serde_json::json;

use std::collections::HashMap;

use mouser_proto::ids::ComputerId;

use crate::notify::{LOST_HOLD, Names, Notice, QUIET_FOR, Toasts, gated};
use crate::support_tests::FakeNotifier;

pub(crate) fn problem_note(cleared: bool) -> Notification {
    note_of(
        "missing_permission:accessibility",
        ProblemKind::MissingPermission,
        "Mouser needs Accessibility access.",
        cleared,
    )
}

const HERE: ComputerId = ComputerId([0; 16]);
const BETA: ComputerId = ComputerId([1; 16]);

fn names() -> Names {
    HashMap::from([(HERE, "Desk Mac".to_string()), (BETA, "Beta".to_string())])
}

fn note_of(id: &str, kind: ProblemKind, message: &str, cleared: bool) -> Notification {
    note_for(HERE, id, kind, message, cleared)
}

fn note_for(
    computer: ComputerId,
    id: &str,
    kind: ProblemKind,
    message: &str,
    cleared: bool,
) -> Notification {
    let problem = Problem {
        problem_id: id.into(),
        kind,
        computer,
        message: message.into(),
        app: None,
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
    attention_on("Desk Mac", body)
}

fn attention_on(computer: &str, body: &str) -> Option<Notice> {
    Some(Notice {
        title: format!("Mouser needs your attention on {computer}"),
        body: body.into(),
        route: None,
    })
}

#[test]
fn a_raised_problem_becomes_a_system_notification() {
    let mut toasts = Toasts::default();
    assert_eq!(
        toasts.notice(&problem_note(false), Instant::now(), &names()),
        attention("Mouser needs Accessibility access.")
    );
}

#[test]
fn a_cleared_problem_and_other_topics_make_no_notification() {
    let mut toasts = Toasts::default();
    let now = Instant::now();
    assert_eq!(toasts.notice(&problem_note(true), now, &names()), None);
    let status = Notification {
        jsonrpc: "2.0".into(),
        method: Topic::Status,
        params: json!({}),
    };
    assert_eq!(toasts.notice(&status, now, &names()), None);
}

#[test]
fn only_new_problems_reach_the_notifier() {
    let notifier = FakeNotifier::default();
    let mut toasts = Toasts::default();
    let now = Instant::now();
    toasts.bridge(&problem_note(false), now, &names(), &notifier);
    toasts.bridge(&problem_note(true), now, &names(), &notifier);
    assert_eq!(
        *notifier.shown.lock().unwrap(),
        vec![(
            "Mouser needs your attention on Desk Mac".to_string(),
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
    assert_eq!(toasts.notice(&blocked, Instant::now(), &names()), None);
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
    assert_eq!(toasts.notice(&lost, Instant::now(), &names()), None);
    // Other problems the platform notices (e.g. a firewall block) still do.
    let firewall = note_of(
        "platform_limitation:platform_problem:fedcba9876543210",
        ProblemKind::PlatformLimitation,
        "Windows Firewall is blocking Mouser.",
        false,
    );
    assert_eq!(
        toasts.notice(&firewall, Instant::now(), &names()),
        attention("Windows Firewall is blocking Mouser.")
    );
}

#[test]
fn a_peer_going_offline_still_becomes_a_system_notification_once() {
    let mut toasts = Toasts::default();
    let now = Instant::now();
    let offline = |cleared| {
        note_of(
            "peer_offline:kitchen",
            ProblemKind::PeerOffline,
            "Kitchen is offline.",
            cleared,
        )
    };
    assert_eq!(
        toasts.notice(&offline(false), now, &names()),
        attention("Kitchen is offline.")
    );
    assert_eq!(toasts.notice(&offline(true), now, &names()), None);
    assert_eq!(
        toasts.notice(&offline(false), now, &names()),
        None,
        "quiet spell"
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
    assert!(toasts.notice(&problem_note(false), now, &names()).is_some());
    assert_eq!(
        toasts.notice(&problem_note(false), now + QUIET_FOR * 2, &names()),
        None
    );
}

#[test]
fn a_problem_that_comes_back_stays_quiet_for_a_while() {
    let mut toasts = Toasts::default();
    let start = Instant::now();
    assert!(
        toasts
            .notice(&problem_note(false), start, &names())
            .is_some()
    );
    // Cleared and raised again, a minute apart, for most of the quiet spell.
    let minute = Duration::from_secs(60);
    for i in 1..10 {
        let at = start + minute * i;
        assert_eq!(toasts.notice(&problem_note(true), at, &names()), None);
        assert_eq!(
            toasts.notice(&problem_note(false), at, &names()),
            None,
            "minute {i}"
        );
    }
    assert_eq!(
        toasts.notice(&problem_note(true), start + QUIET_FOR, &names()),
        None
    );
    // Once the quiet spell has passed it may be shown again.
    assert_eq!(
        toasts.notice(&problem_note(false), start + QUIET_FOR, &names()),
        attention("Mouser needs Accessibility access.")
    );
}

#[test]
fn the_quiet_spell_is_per_problem() {
    let mut toasts = Toasts::default();
    let now = Instant::now();
    assert!(toasts.notice(&problem_note(false), now, &names()).is_some());
    let refused = note_of(
        "join_refused:kitchen",
        ProblemKind::JoinRefused,
        "Kitchen refused to join.",
        false,
    );
    assert_eq!(
        toasts.notice(&refused, now, &names()),
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
    gated(&control, &notifier).await.open("#off");
    control
        .call(
            mouser_proto::control::Method::ConfigSet,
            Some(json!({"key": "general.notifications", "value": true})),
        )
        .await
        .unwrap();
    gated(&control, &notifier).await.show("on again", "3");
    gated(&control, &notifier).await.open("#on");
    let shown = notifier.shown.lock().unwrap();
    assert_eq!(
        *shown,
        vec![
            ("on".to_string(), "1".to_string()),
            ("on again".to_string(), "3".to_string())
        ]
    );
    assert_eq!(*notifier.opened.lock().unwrap(), vec!["#on".to_string()]);
}

#[test]
fn a_problem_is_keyed_by_its_id_and_its_computer() {
    let mut toasts = Toasts::default();
    let now = Instant::now();
    let raised = |computer, cleared| {
        note_for(
            computer,
            "missing_permission:accessibility",
            ProblemKind::MissingPermission,
            "Accessibility is missing.",
            cleared,
        )
    };
    assert_eq!(
        toasts.notice(&raised(HERE, false), now, &names()),
        attention_on("Desk Mac", "Accessibility is missing.")
    );
    // The same problem id on another computer is another problem: shown once, naming that computer.
    assert_eq!(
        toasts.notice(&raised(BETA, false), now, &names()),
        attention_on("Beta", "Accessibility is missing.")
    );
    // Neither is shown again while open, and clearing one leaves the other open.
    assert_eq!(toasts.notice(&raised(HERE, false), now, &names()), None);
    assert_eq!(toasts.notice(&raised(BETA, true), now, &names()), None);
    assert_eq!(toasts.notice(&raised(HERE, false), now, &names()), None);
}

#[test]
fn a_computer_the_tray_has_not_heard_of_is_named_as_another_computer() {
    let mut toasts = Toasts::default();
    let stranger = ComputerId([9; 16]);
    let note = note_for(
        stranger,
        "peer_offline:x",
        ProblemKind::PeerOffline,
        "Offline.",
        false,
    );
    assert_eq!(
        toasts.notice(&note, Instant::now(), &names()),
        attention_on("another computer", "Offline.")
    );
}

#[test]
fn the_elevated_app_toast_opens_that_computers_health_entry() {
    let mut toasts = Toasts::default();
    let notifier = FakeNotifier::default();
    let note = note_for(
        BETA,
        "elevated_app:beta",
        ProblemKind::ElevatedApp,
        "Beta cannot be controlled while VMware runs as administrator.",
        false,
    );
    toasts.bridge(&note, Instant::now(), &names(), &notifier);
    assert_eq!(notifier.shown.lock().unwrap().len(), 1);
    assert_eq!(
        *notifier.opened.lock().unwrap(),
        vec![format!("#health/{BETA}")]
    );
    // Other problems only show the notification.
    toasts.bridge(&problem_note(false), Instant::now(), &names(), &notifier);
    assert_eq!(notifier.opened.lock().unwrap().len(), 1);
}

#[test]
fn a_platform_limitation_is_never_special_cased_any_more() {
    // Limitations arrive through the listing, not as problems, so a problem of that
    // kind is treated like any other problem that needs the owner.
    let mut toasts = Toasts::default();
    let note = note_of(
        "platform_limitation:inject",
        ProblemKind::PlatformLimitation,
        "Some games ignore Mouser's input on this PC.",
        false,
    );
    assert_eq!(
        toasts.notice(&note, Instant::now(), &names()),
        attention("Some games ignore Mouser's input on this PC.")
    );
}
