//! The problems registry: user-visible problems, de-duplicated by
//! `problem_id` (kind + subject), announced to subscribers.

pub mod limitations;
pub mod secure_input;

use std::sync::Mutex;

use mouser_proto::control::{FixAction, Notification, Problem, ProblemKind, Topic};
use mouser_proto::ids::ComputerId;
use mouser_proto::model::PermissionKind;
use tokio::sync::broadcast;

/// RFC 3339 UTC timestamp for "now".
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn kind_name(kind: ProblemKind) -> String {
    serde_json::to_value(kind)
        .expect("kind serializes")
        .as_str()
        .expect("kind is a string")
        .to_string()
}

/// Plain-language reason a permission is needed (constitution II).
pub fn permission_text(kind: PermissionKind) -> &'static str {
    match kind {
        PermissionKind::Accessibility => {
            "Mouser needs Accessibility access to move the pointer and type on this computer."
        }
        PermissionKind::InputMonitoring => {
            "Mouser needs Input Monitoring access to see the mouse and keyboard on this computer."
        }
        PermissionKind::ScreenRecording => {
            "Mouser needs Screen Recording access to see where full-screen windows are."
        }
        PermissionKind::FullDiskAccess => {
            "Mouser needs Full Disk Access to copy files to and from this computer."
        }
        PermissionKind::InputCapturePortal => {
            "Mouser needs permission to capture input through the desktop's input portal."
        }
        PermissionKind::RemoteDesktopPortal => {
            "Mouser needs permission to control this computer through the desktop's remote-control portal."
        }
    }
}

pub struct Problems {
    all: Mutex<Vec<Problem>>,
    notify: broadcast::Sender<Notification>,
}

impl Problems {
    pub fn new(notify: broadcast::Sender<Notification>) -> Self {
        Self {
            all: Mutex::new(Vec::new()),
            notify,
        }
    }

    fn announce(&self, problem: &Problem) {
        let note = Notification {
            jsonrpc: "2.0".into(),
            method: Topic::Problem,
            params: serde_json::to_value(problem).expect("problem serializes"),
        };
        // No subscribers is fine: the problem stays in the registry.
        let _ = self.notify.send(note);
    }

    /// Raise a problem; returns false (and says nothing) if the same problem
    /// (kind + subject) is already open.
    pub fn raise(
        &self,
        kind: ProblemKind,
        subject: &str,
        computer: ComputerId,
        message: String,
        fix: Option<FixAction>,
    ) -> bool {
        let problem_id = format!("{}:{subject}", kind_name(kind));
        let mut all = self.all.lock().unwrap();
        if all
            .iter()
            .any(|p| p.problem_id == problem_id && p.cleared_at.is_none())
        {
            return false;
        }
        let problem = Problem {
            problem_id,
            kind,
            computer,
            message,
            fix,
            raised_at: now(),
            cleared_at: None,
        };
        all.push(problem.clone());
        drop(all);
        self.announce(&problem);
        true
    }

    /// Clear an open problem; returns whether there was one.
    pub fn clear(&self, kind: ProblemKind, subject: &str) -> bool {
        let problem_id = format!("{}:{subject}", kind_name(kind));
        let mut all = self.all.lock().unwrap();
        let Some(problem) = all
            .iter_mut()
            .find(|p| p.problem_id == problem_id && p.cleared_at.is_none())
        else {
            return false;
        };
        problem.cleared_at = Some(now());
        let cleared = problem.clone();
        drop(all);
        self.announce(&cleared);
        true
    }

    /// Problems not yet cleared.
    pub fn open(&self) -> Vec<Problem> {
        let all = self.all.lock().unwrap();
        all.iter()
            .filter(|p| p.cleared_at.is_none())
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod limitations_tests;
#[cfg(test)]
mod secure_input_tests;
#[cfg(test)]
mod tests;
