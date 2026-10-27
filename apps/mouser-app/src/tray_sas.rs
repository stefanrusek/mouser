//! "Send Ctrl-Alt-Del" (Story 12 scenario 4): the tray line that asks the
//! engine to send the secure attention sequence to the active computer.

use mouser_core::ControlHandle;
use mouser_proto::control::{Method, Status};
use mouser_proto::model::Os;

use crate::notify::Notifier;
use crate::tray::Entry;

pub const SEND_CAD_ID: &str = "send_cad";
pub const TITLE: &str = "Mouser could not send Ctrl-Alt-Del";

/// Whether the computer the pointer is on (this one when the pointer is at
/// home) is a Windows computer, the only kind that can receive the sequence.
pub fn shows_send_cad(status: &Status) -> bool {
    let active = status.active_computer;
    status
        .computers
        .iter()
        .any(|c| c.id == active && c.os == Os::Windows)
}

/// The tray line, present only while [`shows_send_cad`].
pub fn entry(status: &Status) -> Option<Entry> {
    shows_send_cad(status).then(|| Entry {
        id: Some(SEND_CAD_ID),
        label: "Send Ctrl-Alt-Del".into(),
    })
}

/// Call `send_cad`; the error is the engine's plain-language message.
pub async fn send(control: &ControlHandle) -> Result<(), String> {
    control
        .call(Method::SendCad, None)
        .await
        .map(|_| ())
        .map_err(|e| e.message)
}

/// Tell the owner why the sequence was not sent.
pub fn report(result: Result<(), String>, notifier: &dyn Notifier) {
    if let Err(message) = result {
        notifier.show(TITLE, &message);
    }
}
