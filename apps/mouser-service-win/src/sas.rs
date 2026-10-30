//! Ctrl-Alt-Del for the session engine (research V10): `SendSAS` works only
//! from a service, and only where `SoftwareSASGeneration` allows services.

use mouser_core::logging::{Log, events};
use mouser_proto::service_ipc::SasResult;

use crate::winapi::{ERROR_FILE_NOT_FOUND, WinApi};

pub const POLICY_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\System";
pub const POLICY_VALUE: &str = "SoftwareSASGeneration";

/// Sends Ctrl-Alt-Del when the policy lets services (1) or both services
/// and accessibility applications (3) do it, and logs the outcome.
pub fn answer(api: &dyn WinApi, log: &Log) -> SasResult {
    let result = match api.read_dword(POLICY_KEY, POLICY_VALUE) {
        Ok(1 | 3) => match api.send_sas() {
            Ok(()) => SasResult::Sent,
            Err(e) => SasResult::Failed(e.to_string()),
        },
        Ok(_) => SasResult::PolicyBlocked,
        Err(e) if e.code == ERROR_FILE_NOT_FOUND => SasResult::PolicyBlocked,
        Err(e) => SasResult::Failed(e.to_string()),
    };
    log.write(events::service_sas(&result));
    result
}
