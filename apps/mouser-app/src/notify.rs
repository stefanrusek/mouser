//! The OS-notification bridge: raised problems also reach the owner as a
//! system notification.

use mouser_core::ControlHandle;
use mouser_proto::control::{Method, Notification, Problem, Topic};
use serde_json::json;

/// Shows a system notification.
pub trait Notifier: Send + Sync {
    fn show(&self, title: &str, body: &str);
}

/// A system notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
}

/// The notification for a newly raised problem; none for other topics or
/// for a problem that was just cleared.
pub fn notice(note: &Notification) -> Option<Notice> {
    if note.method != Topic::Problem {
        return None;
    }
    let problem: Problem =
        serde_json::from_value(note.params.clone()).expect("problem matches its schema");
    problem.cleared_at.is_none().then(|| Notice {
        title: "Mouser needs your attention".into(),
        body: problem.message,
    })
}

/// Show `note` as a system notification if it is a new problem.
pub fn bridge(note: &Notification, notifier: &dyn Notifier) {
    if let Some(notice) = notice(note) {
        notifier.show(&notice.title, &notice.body);
    }
}

/// Shows nothing: `general.notifications` is off.
struct Silent;

impl Notifier for Silent {
    fn show(&self, _title: &str, _body: &str) {}
}

/// `notifier`, or a silent one when `general.notifications` is false right
/// now (read each time, so `config.set` and file edits apply live).
/// Problems and join requests still show in their own windows.
pub async fn gated<'a>(control: &ControlHandle, notifier: &'a dyn Notifier) -> &'a dyn Notifier {
    let on = control
        .call(
            Method::ConfigGet,
            Some(json!({"key": "general.notifications"})),
        )
        .await
        .expect("the notifications setting can be read");
    if on.as_bool().expect("notifications is a boolean") {
        notifier
    } else {
        &Silent
    }
}
