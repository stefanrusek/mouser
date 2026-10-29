//! Diagnostics: lines for the owner's diagnostic log that help tell causes
//! apart later (which hook missed a probe, what was in front, why the
//! capture was lost). The platform seam carries only problems, so the app
//! hands this backend a sink that writes to the log.
//!
//! What this backend can write is closed: [`DiagEvent`] has one variant per
//! event of the catalog (contracts/log-events.md) that it writes, with that
//! event's fields, typed, and its level.

use mouser_platform::CaptureLossReason;
use std::sync::{Arc, RwLock};

/// The level of a line, as the catalog gives it per event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagLevel {
    Debug,
    Info,
    Warn,
}

/// The value of a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagValue {
    Str(String),
    Bool(bool),
    Int(i64),
}

impl std::fmt::Display for DiagValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiagValue::Str(s) => f.write_str(s),
            DiagValue::Bool(b) => write!(f, "{b}"),
            DiagValue::Int(i) => write!(f, "{i}"),
        }
    }
}

/// Takes a line's level, its event name and its fields.
pub type Diagnostics = Arc<dyn Fn(DiagLevel, &str, Vec<(&'static str, DiagValue)>) + Send + Sync>;

/// A low-level hook, as a line names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagHook {
    Mouse,
    Keyboard,
}

impl DiagHook {
    fn name(self) -> &'static str {
        match self {
            DiagHook::Mouse => "mouse",
            DiagHook::Keyboard => "keyboard",
        }
    }
}

/// What became of a watchdog probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    /// Both hooks saw it within its wait.
    Seen,
    /// A hook did not see it within its wait.
    Unseen,
    /// A hook saw it after its wait.
    Late,
    /// A hook saw nothing though someone used the PC.
    Missed,
    /// There was a reason not to probe.
    Skipped,
    /// The system would not take the probe.
    NotSent,
    /// The hooks saw real input, which shows they work.
    Live,
}

impl ProbeOutcome {
    fn name(self) -> &'static str {
        match self {
            ProbeOutcome::Seen => "seen",
            ProbeOutcome::Unseen => "unseen",
            ProbeOutcome::Late => "late",
            ProbeOutcome::Missed => "missed",
            ProbeOutcome::Skipped => "skipped",
            ProbeOutcome::NotSent => "not_sent",
            ProbeOutcome::Live => "live",
        }
    }
}

/// A `capture.probe` line: what one watchdog cycle found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Probe {
    pub outcome: ProbeOutcome,
    /// The hook the outcome is about, when it is about one.
    pub hook: Option<DiagHook>,
    /// How many misses in a row, for `Missed`.
    pub misses: Option<u32>,
    /// Why no probe was sent, for `Skipped`.
    pub reason: Option<&'static str>,
    /// The program in front, by executable name, when it can be named.
    pub foreground: Option<String>,
}

impl Probe {
    /// The line's fields, in this order: outcome, hook, misses, reason,
    /// foreground; those not set are left out.
    pub fn fields(&self) -> Vec<(&'static str, DiagValue)> {
        let mut fields = vec![("outcome", DiagValue::Str(self.outcome.name().into()))];
        if let Some(hook) = self.hook {
            fields.push(("hook", DiagValue::Str(hook.name().into())));
        }
        if let Some(misses) = self.misses {
            fields.push(("misses", DiagValue::Int(i64::from(misses))));
        }
        if let Some(reason) = self.reason {
            fields.push(("reason", DiagValue::Str(reason.into())));
        }
        if let Some(foreground) = &self.foreground {
            fields.push(("foreground", DiagValue::Str(foreground.clone())));
        }
        fields
    }
}

/// Every line this backend writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagEvent {
    /// `pointer.shown {reason: guard}`: the cursor guard put the pointer
    /// back after Mouser ended without doing so.
    PointerShownByGuard,
    /// `capture.lost`: the capture could no longer see input and let go.
    CaptureLost {
        reason: CaptureLossReason,
        /// The program in front, by executable name, when it can be named.
        foreground: Option<String>,
    },
    /// `capture.start`, and whether power throttling is off for Mouser.
    CaptureStart { unthrottled: bool },
    /// `capture.hook_thread`, and whether it runs time-critical.
    CaptureHookThread { time_critical: bool },
    /// `capture.reinstall` of a hook, and whether it was installed again.
    CaptureReinstall { hook: DiagHook, ok: bool },
    /// `capture.probe`.
    CaptureProbe(Probe),
}

impl DiagEvent {
    /// The event's name in the catalog.
    pub fn name(&self) -> &'static str {
        match self {
            DiagEvent::PointerShownByGuard => "pointer.shown",
            DiagEvent::CaptureLost { .. } => "capture.lost",
            DiagEvent::CaptureStart { .. } => "capture.start",
            DiagEvent::CaptureHookThread { .. } => "capture.hook_thread",
            DiagEvent::CaptureReinstall { .. } => "capture.reinstall",
            DiagEvent::CaptureProbe(_) => "capture.probe",
        }
    }

    /// The event's level in the catalog.
    pub fn level(&self) -> DiagLevel {
        match self {
            DiagEvent::PointerShownByGuard => DiagLevel::Info,
            DiagEvent::CaptureLost { .. } => DiagLevel::Warn,
            _ => DiagLevel::Debug,
        }
    }

    /// The event's fields; optional ones not set are left out.
    pub fn fields(&self) -> Vec<(&'static str, DiagValue)> {
        let text = |s: &str| DiagValue::Str(s.into());
        match self {
            DiagEvent::PointerShownByGuard => vec![("reason", text("guard"))],
            DiagEvent::CaptureLost { reason, foreground } => {
                let reason = match reason {
                    CaptureLossReason::ElevatedForeground => "elevated_foreground",
                    CaptureLossReason::OtherDesktop => "other_desktop",
                    CaptureLossReason::HooksSilent => "hooks_silent",
                };
                let mut fields = vec![("reason", text(reason))];
                if let Some(foreground) = foreground {
                    fields.push(("foreground", text(foreground)));
                }
                fields
            }
            DiagEvent::CaptureStart { unthrottled } => {
                vec![("unthrottled", DiagValue::Bool(*unthrottled))]
            }
            DiagEvent::CaptureHookThread { time_critical } => {
                vec![("time_critical", DiagValue::Bool(*time_critical))]
            }
            DiagEvent::CaptureReinstall { hook, ok } => {
                vec![("hook", text(hook.name())), ("ok", DiagValue::Bool(*ok))]
            }
            DiagEvent::CaptureProbe(probe) => probe.fields(),
        }
    }
}

static SINK: RwLock<Option<Diagnostics>> = RwLock::new(None);

/// Where this backend's diagnostics go from now on; `None` drops them (the
/// default).
pub fn set_diagnostics(sink: Option<Diagnostics>) {
    *SINK.write().unwrap() = sink;
}

/// Writes one line, if anyone listens.
pub(crate) fn note(event: DiagEvent) {
    if let Some(sink) = SINK.read().unwrap().as_ref() {
        sink(event.level(), event.name(), event.fields());
    }
}
