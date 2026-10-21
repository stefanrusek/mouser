//! The OS-notification bridge: raised problems also reach the owner as a
//! system notification.

use mouser_proto::control::{Notification, Problem, Topic};

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
