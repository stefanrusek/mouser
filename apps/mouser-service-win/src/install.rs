//! `mouser-service.exe --install` and `--uninstall`, run elevated by the
//! installer's hooks (research V12, FR-013): the machine-wide parts of
//! Mouser are put in place, and taken away so that nothing of Mouser keeps
//! running.

use std::io::Write;
use std::path::Path;

use crate::launch::ENGINE;
use crate::sas::{POLICY_KEY, POLICY_VALUE};
use crate::winapi::{
    Account, ApiError, ApiResult, ERROR_FILE_NOT_FOUND, ERROR_SERVICE_DOES_NOT_EXIST,
    ERROR_SERVICE_MARKED_FOR_DELETE, ERROR_SERVICE_NOT_ACTIVE, FirewallRule, ServiceSpec,
    StartType, WinApi,
};

pub const SERVICE_NAME: &str = "MouserService";
pub const DISPLAY_NAME: &str = "Mouser";
/// The service's executable, beside the engine's.
pub const SERVICE_EXE: &str = "mouser-service.exe";
/// The inbound firewall rule's name.
pub const RULE_NAME: &str = "Mouser";
/// Discovery and links (001 contracts).
pub const UDP_PORT: u16 = 47801;
/// The machine secrets folder: SYSTEM and Administrators only, nothing
/// inherited.
pub const SECRETS_SDDL: &str = "D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
/// `ERROR_SERVICE_ALREADY_RUNNING`.
const ERROR_SERVICE_ALREADY_RUNNING: u32 = 1056;
/// `ERROR_SERVICE_EXISTS`.
const ERROR_SERVICE_EXISTS: u32 = 1073;
/// `ERROR_INVALID_PARAMETER`: the process has ended.
const ERROR_INVALID_PARAMETER: u32 = 87;

/// `result`, unless it failed with one of `done`, which means the step's
/// work is there already.
fn unless(result: ApiResult<()>, done: &[u32]) -> ApiResult<()> {
    match result {
        Err(e) if done.contains(&e.code) => Ok(()),
        other => other,
    }
}

/// The step named `what` failed: its reason.
fn step(what: &str, e: ApiError) -> String {
    format!("{what}: {e}")
}

/// Puts Mouser's machine-wide parts in place from `dir`, the folder both
/// executables are in, with the machine folder `machine`, and starts the
/// service last. `migrate` copies the installing user's identity there
/// (research V12a); when it cannot, the session engine starts without a desk
/// and the computer rejoins with one approval (OD-2), so that is no failure.
pub fn install(
    api: &dyn WinApi,
    dir: &Path,
    machine: &Path,
    migrate: &dyn Fn() -> Result<(), String>,
) -> Result<(), String> {
    let spec = ServiceSpec {
        name: SERVICE_NAME.into(),
        display_name: DISPLAY_NAME.into(),
        executable: dir.join(SERVICE_EXE),
        account: Account::LocalSystem,
        start: StartType::Automatic,
    };
    unless(api.create_service(&spec), &[ERROR_SERVICE_EXISTS])
        .map_err(|e| step("creating MouserService", e))?;
    api.write_dword(POLICY_KEY, POLICY_VALUE, 1)
        .map_err(|e| step("setting SoftwareSASGeneration", e))?;
    add_rule(api, dir).map_err(|e| step("adding the firewall rule", e))?;
    // Machine-scope DPAPI keeps the keys from other computers only: this
    // folder's DACL keeps them from this computer's other users.
    let secrets = machine.join("secrets");
    api.protect_dir(&secrets, SECRETS_SDDL)
        .map_err(|e| format!("protecting {}: {e}", secrets.display()))?;
    let _ = migrate();
    unless(
        api.start_service(SERVICE_NAME),
        &[ERROR_SERVICE_ALREADY_RUNNING],
    )
    .map_err(|e| step("starting MouserService", e))
}

fn add_rule(api: &dyn WinApi, dir: &Path) -> ApiResult<()> {
    if api.has_firewall_rule(RULE_NAME)? {
        return Ok(());
    }
    api.add_firewall_rule(&FirewallRule {
        name: RULE_NAME.into(),
        program: dir.join(ENGINE),
        udp_port: UDP_PORT,
    })
}

/// Takes Mouser's machine-wide parts away; `me` is this process, itself a
/// `mouser-service.exe`. What is gone already counts as done.
pub fn uninstall(api: &dyn WinApi, me: u32) -> Result<(), String> {
    stop_service(api, me).map_err(|e| step("stopping MouserService", e))?;
    unless(
        api.delete_service(SERVICE_NAME),
        &[
            ERROR_SERVICE_DOES_NOT_EXIST,
            ERROR_SERVICE_MARKED_FOR_DELETE,
        ],
    )
    .map_err(|e| step("deleting MouserService", e))?;
    end_engines(api).map_err(|e| step("ending mouser-app.exe", e))?;
    remove_rule(api).map_err(|e| step("removing the firewall rule", e))?;
    unless(
        api.delete_value(POLICY_KEY, POLICY_VALUE),
        &[ERROR_FILE_NOT_FOUND],
    )
    .map_err(|e| step("removing SoftwareSASGeneration", e))
}

/// Stops the service and waits for its process to end: it shuts its
/// session engine down first.
fn stop_service(api: &dyn WinApi, me: u32) -> ApiResult<()> {
    unless(
        api.stop_service(SERVICE_NAME),
        &[ERROR_SERVICE_NOT_ACTIVE, ERROR_SERVICE_DOES_NOT_EXIST],
    )?;
    for process in api.processes()? {
        if process.pid == me || !process.name.eq_ignore_ascii_case(SERVICE_EXE) {
            continue;
        }
        // One that cannot be opened has ended.
        if let Ok(handle) = api.open_process(process.pid) {
            let _ = api.wait_process(handle);
            api.close(handle);
        }
    }
    Ok(())
}

/// Ends every `mouser-app.exe` left in any session: tray apps in UI mode and
/// cursor guards.
fn end_engines(api: &dyn WinApi) -> ApiResult<()> {
    for process in api.processes()? {
        if process.name.eq_ignore_ascii_case(ENGINE) {
            unless(api.terminate(process.pid), &[ERROR_INVALID_PARAMETER])?;
        }
    }
    Ok(())
}

fn remove_rule(api: &dyn WinApi) -> ApiResult<()> {
    if !api.has_firewall_rule(RULE_NAME)? {
        return Ok(());
    }
    api.delete_firewall_rule(RULE_NAME)
}

/// The exit code of `--install` or `--uninstall`: 0, or 1 with the failed
/// step's reason on `stderr`.
pub fn exit_code(result: Result<(), String>, stderr: &mut dyn Write) -> i32 {
    match result {
        Ok(()) => 0,
        Err(reason) => {
            let _ = writeln!(stderr, "{reason}");
            1
        }
    }
}
