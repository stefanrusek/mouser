//! `MouserService` under the SCM (research V9): the controls it accepts
//! move the state machine, and the SCM hears it running and stopped. The
//! registration itself is `winapi_sys`'s.

use std::sync::mpsc::{Receiver, Sender};

use crate::session::{Event, Supervisor};

/// Why a `SERVICE_CONTROL_SESSIONCHANGE` came.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionReason {
    ConsoleConnect,
    ConsoleDisconnect,
    Logon,
    Logoff,
    /// Lock, unlock, remote sessions: the engine follows the desktop itself.
    Other,
}

/// A control the SCM sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Stop,
    Preshutdown,
    SessionChange(SessionReason),
    Interrogate,
    Other,
}

/// What the service tells the SCM it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reported {
    Running,
    Stopped,
}

pub trait Scm {
    fn report(&self, state: Reported);
}

/// Hands `control` to the state machine through `events`; false for a
/// control the service does not implement.
pub fn dispatch(control: Control, events: &Sender<Event>) -> bool {
    let event = match control {
        Control::Stop | Control::Preshutdown => Event::Stop,
        Control::SessionChange(SessionReason::Other) | Control::Interrogate => return true,
        Control::SessionChange(_) => Event::SessionChange,
        Control::Other => return false,
    };
    // A supervisor that has stopped hears nothing more.
    let _ = events.send(event);
    true
}

/// The service's life: running while `supervisor` runs on `events`.
pub fn serve(scm: &dyn Scm, mut supervisor: Supervisor, events: Receiver<Event>) {
    scm.report(Reported::Running);
    supervisor.run(events);
    scm.report(Reported::Stopped);
}
