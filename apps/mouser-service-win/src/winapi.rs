//! The seam between `MouserService` and Windows (OD-5). Everything the
//! service asks of the system goes through [`WinApi`]; the real one,
//! `winapi_sys`, is one call per method, and the tests use a fake.

use std::fmt;
use std::path::{Path, PathBuf};

/// A failed Windows call: its error code and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub code: u32,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (error {})", self.message, self.code)
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

/// `ERROR_FILE_NOT_FOUND`: a registry value that is not there.
pub const ERROR_FILE_NOT_FOUND: u32 = 2;
/// `ERROR_SERVICE_DOES_NOT_EXIST`.
pub const ERROR_SERVICE_DOES_NOT_EXIST: u32 = 1060;
/// `ERROR_SERVICE_NOT_ACTIVE`: stopping a service that is not running.
pub const ERROR_SERVICE_NOT_ACTIVE: u32 = 1062;
/// `ERROR_SERVICE_MARKED_FOR_DELETE`: deleted already, gone once closed.
pub const ERROR_SERVICE_MARKED_FOR_DELETE: u32 = 1072;

/// A kernel object the caller owns until it hands it to [`WinApi::close`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Handle(pub isize);

/// A process [`WinApi::create_process_as_user`] started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Launched {
    pub pid: u32,
    pub process: Handle,
}

/// One process of any session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub session: u32,
    /// The image name, such as `winlogon.exe`.
    pub name: String,
}

/// The account a service runs as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Account {
    LocalSystem,
}

/// When the SCM starts a service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartType {
    Automatic,
}

/// A service to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSpec {
    pub name: String,
    pub display_name: String,
    pub executable: PathBuf,
    pub account: Account,
    pub start: StartType,
}

/// An inbound firewall rule for one UDP port of one program.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirewallRule {
    pub name: String,
    pub program: PathBuf,
    pub udp_port: u16,
}

pub trait WinApi: Send + Sync {
    /// `WTSGetActiveConsoleSessionId`: `0xFFFFFFFF` while there is none.
    fn console_session(&self) -> u32;
    /// `WTSEnumerateProcessesW`: the processes of every session.
    fn processes(&self) -> ApiResult<Vec<ProcessEntry>>;
    /// A primary token duplicated from process `pid`'s token
    /// (`OpenProcess`, `OpenProcessToken`, `DuplicateTokenEx`).
    fn duplicate_token(&self, pid: u32) -> ApiResult<Handle>;
    /// `CreateProcessAsUserW` with `token`, running `command_line` of
    /// `application` on `desktop`.
    fn create_process_as_user(
        &self,
        token: Handle,
        application: &Path,
        command_line: &str,
        desktop: &str,
    ) -> ApiResult<Launched>;
    /// `OpenProcess` for waiting on process `pid`.
    fn open_process(&self, pid: u32) -> ApiResult<Handle>;
    /// Waits for `process` to end and gives its exit code.
    fn wait_process(&self, process: Handle) -> ApiResult<u32>;
    /// `TerminateProcess` on process `pid`.
    fn terminate(&self, pid: u32) -> ApiResult<()>;
    /// `CloseHandle`.
    fn close(&self, handle: Handle);

    /// `CreateNamedPipeW` with the security descriptor `sddl`.
    fn create_pipe(
        &self,
        name: &str,
        sddl: &str,
        open_mode: u32,
        pipe_mode: u32,
    ) -> ApiResult<Handle>;
    /// `ConnectNamedPipe`: waits for a client. `ERROR_PIPE_CONNECTED` (535)
    /// means one connected before the call.
    fn connect_pipe(&self, pipe: Handle) -> ApiResult<()>;
    /// `GetNamedPipeClientProcessId`.
    fn pipe_client_pid(&self, pipe: Handle) -> ApiResult<u32>;
    /// `DisconnectNamedPipe`.
    fn disconnect_pipe(&self, pipe: Handle) -> ApiResult<()>;
    /// `CreateFileW` on the pipe `name` as its client: ends a wait in
    /// [`connect_pipe`](Self::connect_pipe), even one not begun yet.
    fn open_pipe_client(&self, name: &str) -> ApiResult<Handle>;
    /// `ReadFile`: some bytes; an error once the client has gone.
    fn read_pipe(&self, pipe: Handle, buf: &mut [u8]) -> ApiResult<usize>;
    /// `WriteFile`, all of `bytes`.
    fn write_pipe(&self, pipe: Handle, bytes: &[u8]) -> ApiResult<()>;

    /// Creates the folder `path` if need be and gives it the DACL `sddl`,
    /// protected from what it would inherit (`SetNamedSecurityInfoW`).
    fn protect_dir(&self, path: &Path, sddl: &str) -> ApiResult<()>;

    /// `SendSAS(FALSE)`.
    fn send_sas(&self) -> ApiResult<()>;

    /// `RegGetValueW` of a DWORD under `HKEY_LOCAL_MACHINE`.
    fn read_dword(&self, key: &str, value: &str) -> ApiResult<u32>;
    /// `RegSetKeyValueW` of a DWORD under `HKEY_LOCAL_MACHINE`.
    fn write_dword(&self, key: &str, value: &str, data: u32) -> ApiResult<()>;
    /// `RegDeleteKeyValueW` under `HKEY_LOCAL_MACHINE`.
    fn delete_value(&self, key: &str, value: &str) -> ApiResult<()>;

    /// `CreateServiceW`.
    fn create_service(&self, spec: &ServiceSpec) -> ApiResult<()>;
    /// `StartServiceW`.
    fn start_service(&self, name: &str) -> ApiResult<()>;
    /// `ControlService(SERVICE_CONTROL_STOP)`; the service goes on stopping
    /// after this returns.
    fn stop_service(&self, name: &str) -> ApiResult<()>;
    /// `DeleteService`.
    fn delete_service(&self, name: &str) -> ApiResult<()>;

    /// Adds the inbound rule (`netsh advfirewall firewall add rule`).
    fn add_firewall_rule(&self, rule: &FirewallRule) -> ApiResult<()>;
    /// Whether a rule named `name` exists.
    fn has_firewall_rule(&self, name: &str) -> ApiResult<bool>;
    /// Deletes the rules named `name`.
    fn delete_firewall_rule(&self, name: &str) -> ApiResult<()>;
}
