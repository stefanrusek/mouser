//! The OS-notification bridge: a raised problem that needs the owner to act
//! also reaches them as a system notification, once per raise and at most
//! once per [`QUIET_FOR`]. Every problem still shows in the health panel.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use mouser_core::ControlHandle;
use mouser_proto::control::{Method, Notification, Problem, ProblemKind, Topic};
use mouser_proto::ids::ComputerId;
use serde_json::json;

/// Shows a system notification.
pub trait Notifier: Send + Sync {
    fn show(&self, title: &str, body: &str);
    /// Bring the Mouser window to `route`, for a notification about something the
    /// owner should look at there.
    fn open(&self, route: &str);
}

/// The name of each computer on the desk, from the latest `status`.
pub type Names = HashMap<ComputerId, String>;

/// A system notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub title: String,
    pub body: String,
    /// Where in the window the notification leads, if anywhere.
    pub route: Option<String>,
}

/// How long a problem that was shown stays quiet, even if it clears and is
/// raised again meanwhile.
pub const QUIET_FOR: Duration = Duration::from_secs(10 * 60);

/// What the Windows capture watchdog says after it took the hooks back. It
/// healed by itself, so it is left to the health panel.
pub const LOST_HOLD: &str = "Mouser lost its hold on the keyboard and mouse and took it back; \
                         input may have briefly leaked to this PC.";

/// Whether `problem` needs the owner to act, which is what a system
/// notification is for. A blocked crossing already shows where it happened and
/// a hold the watchdog took back has fixed itself; both stay in Health and the log.
fn needs_owner(problem: &Problem) -> bool {
    problem.kind != ProblemKind::CrossingBlocked && problem.message != LOST_HOLD
}

/// Which problems were shown, so none is shown twice while it stays open
/// and none again within [`QUIET_FOR`] of the last time it was shown.
#[derive(Default)]
pub struct Toasts {
    /// Problems raised and not yet cleared.
    open: HashSet<(String, ComputerId)>,
    /// When each problem was last shown.
    shown: HashMap<(String, ComputerId), Instant>,
}

impl Toasts {
    /// The notification for `note` arriving at `now`: one for a problem that
    /// was just raised, needs the owner, and was not shown lately; none for
    /// anything else. It names the computer it concerns (from `names`).
    pub fn notice(&mut self, note: &Notification, now: Instant, names: &Names) -> Option<Notice> {
        if note.method != Topic::Problem {
            return None;
        }
        let problem: Problem =
            serde_json::from_value(note.params.clone()).expect("problem matches its schema");
        let key = (problem.problem_id.clone(), problem.computer);
        if problem.cleared_at.is_some() {
            self.open.remove(&key);
            return None;
        }
        let newly_raised = self.open.insert(key.clone());
        let lately = self
            .shown
            .get(&key)
            .is_some_and(|at| now.duration_since(*at) < QUIET_FOR);
        if !newly_raised || lately || !needs_owner(&problem) {
            return None;
        }
        self.shown.insert(key, now);
        let name = names
            .get(&problem.computer)
            .map_or("another computer", String::as_str);
        Some(Notice {
            title: format!("Mouser needs your attention on {name}"),
            body: problem.message,
            route: (problem.kind == ProblemKind::ElevatedApp)
                .then(|| format!("#health/{}", problem.computer)),
        })
    }

    /// Show `note`, arriving at `now`, as a system notification if
    /// [`Toasts::notice`] makes one of it.
    pub fn bridge(
        &mut self,
        note: &Notification,
        now: Instant,
        names: &Names,
        notifier: &dyn Notifier,
    ) {
        if let Some(notice) = self.notice(note, now, names) {
            notifier.show(&notice.title, &notice.body);
            if let Some(route) = notice.route {
                notifier.open(&route);
            }
        }
    }
}

/// Shows nothing: `general.notifications` is off.
struct Silent;

impl Notifier for Silent {
    fn show(&self, _title: &str, _body: &str) {}
    fn open(&self, _route: &str) {}
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
