//! The real [`WinApi`]: each method is one call to the Windows API it names,
//! with its error mapped to [`ApiError`]. Nothing else lives here; it runs
//! only as SYSTEM, so it is excluded from coverage (OD-5) and checked by
//! probe P2 and the manual rows R8/R9.

use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::sync::mpsc::channel;
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_IO_PENDING, GENERIC_READ, GENERIC_WRITE, HANDLE, HLOCAL, LocalFree,
    WIN32_ERROR,
};
use windows::Win32::Security::Authentication::Identity::SendSAS;
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, DuplicateTokenEx, GetSecurityDescriptorDacl,
    PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
    SecurityImpersonation, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE, TokenPrimary,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, OPEN_EXISTING, ReadFile, WriteFile,
};
use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    NAMED_PIPE_MODE,
};
use windows::Win32::System::Registry::{
    HKEY_LOCAL_MACHINE, REG_DWORD, RRF_RT_REG_DWORD, RegDeleteKeyValueW, RegGetValueW,
    RegSetKeyValueW,
};
use windows::Win32::System::RemoteDesktop::{
    WTS_PROCESS_INFOW, WTSEnumerateProcessesW, WTSFreeMemory, WTSGetActiveConsoleSessionId,
};
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateEventW, CreateProcessAsUserW, GetExitCodeProcess, INFINITE,
    OpenProcess, OpenProcessToken, PROCESS_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR};
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceAccess, ServiceControl, ServiceControlAccept, ServiceErrorControl, ServiceExitCode,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType, SessionChangeReason,
};
use windows_service::service_control_handler::{
    self, ServiceControlHandlerResult, ServiceStatusHandle,
};
use windows_service::service_dispatcher;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

use crate::install::SERVICE_NAME;
use crate::machine::Started;
use crate::service::{Control, Reported, Scm, SessionReason, dispatch};
use crate::winapi::{
    Account, ApiError, ApiResult, FirewallRule, Handle, Launched, ProcessEntry, ServiceSpec,
    StartType, WinApi,
};

pub struct System;

fn api(e: windows::core::Error) -> ApiError {
    ApiError {
        code: WIN32_ERROR::from_error(&e).map_or(e.code().0 as u32, |w| w.0),
        message: e.message(),
    }
}

fn win32(e: WIN32_ERROR) -> ApiResult<()> {
    e.ok().map_err(api)
}

fn scm(e: windows_service::Error) -> ApiError {
    let code = match &e {
        windows_service::Error::Winapi(io) => io.raw_os_error().unwrap_or(0) as u32,
        _ => 0,
    };
    ApiError {
        code,
        message: e.to_string(),
    }
}

fn netsh(args: &[String]) -> ApiResult<bool> {
    let out = Command::new("netsh")
        .args(["advfirewall", "firewall"])
        .args(args)
        .output()
        .map_err(|e| ApiError {
            code: e.raw_os_error().unwrap_or(0) as u32,
            message: e.to_string(),
        })?;
    Ok(out.status.success())
}

fn netsh_ok(args: &[String]) -> ApiResult<()> {
    match netsh(args)? {
        true => Ok(()),
        false => Err(ApiError {
            code: 1,
            message: format!("netsh advfirewall firewall {} failed", args[0]),
        }),
    }
}

fn raw(h: Handle) -> HANDLE {
    HANDLE(h.0 as *mut _)
}

fn ours(h: HANDLE) -> Handle {
    Handle(h.0 as isize)
}

fn wide(s: &str) -> Vec<u16> {
    OsString::from(s).encode_wide().chain([0]).collect()
}

/// Runs one overlapped operation on `handle` to its end: the bytes moved.
fn overlapped(
    handle: HANDLE,
    start: impl FnOnce(*mut OVERLAPPED) -> windows::core::Result<()>,
) -> ApiResult<u32> {
    let event = unsafe { CreateEventW(None, true, false, None) }.map_err(api)?;
    let mut ov = OVERLAPPED {
        hEvent: event,
        ..Default::default()
    };
    let mut moved = 0;
    let done = match start(&mut ov) {
        Err(e) if e.code() != ERROR_IO_PENDING.to_hresult() => Err(api(e)),
        _ => unsafe { GetOverlappedResult(handle, &ov, &mut moved, true) }.map_err(api),
    };
    let _ = unsafe { CloseHandle(event) };
    done.map(|()| moved)
}

impl WinApi for System {
    fn console_session(&self) -> u32 {
        unsafe { WTSGetActiveConsoleSessionId() }
    }

    fn processes(&self) -> ApiResult<Vec<ProcessEntry>> {
        let mut info: *mut WTS_PROCESS_INFOW = std::ptr::null_mut();
        let mut count = 0;
        unsafe { WTSEnumerateProcessesW(None, 0, 1, &mut info, &mut count) }.map_err(api)?;
        let list = unsafe { std::slice::from_raw_parts(info, count as usize) }
            .iter()
            .map(|p| ProcessEntry {
                pid: p.ProcessId,
                session: p.SessionId,
                name: unsafe { p.pProcessName.to_string() }.unwrap_or_default(),
            })
            .collect();
        unsafe { WTSFreeMemory(info.cast()) };
        Ok(list)
    }

    fn duplicate_token(&self, pid: u32) -> ApiResult<Handle> {
        let process =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.map_err(api)?;
        let mut token = HANDLE::default();
        let opened = unsafe { OpenProcessToken(process, TOKEN_DUPLICATE, &mut token) };
        let _ = unsafe { CloseHandle(process) };
        opened.map_err(api)?;
        let mut primary = HANDLE::default();
        let duplicated = unsafe {
            DuplicateTokenEx(
                token,
                TOKEN_ALL_ACCESS,
                None,
                SecurityImpersonation,
                TokenPrimary,
                &mut primary,
            )
        };
        let _ = unsafe { CloseHandle(token) };
        duplicated.map_err(api)?;
        Ok(ours(primary))
    }

    fn create_process_as_user(
        &self,
        token: Handle,
        application: &Path,
        command_line: &str,
        desktop: &str,
    ) -> ApiResult<Launched> {
        let mut desktop = wide(desktop);
        let mut command_line = wide(command_line);
        let startup = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop.as_mut_ptr()),
            ..Default::default()
        };
        let mut info = PROCESS_INFORMATION::default();
        unsafe {
            CreateProcessAsUserW(
                Some(raw(token)),
                &HSTRING::from(application),
                Some(PWSTR(command_line.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_UNICODE_ENVIRONMENT,
                None,
                PCWSTR::null(),
                &startup,
                &mut info,
            )
        }
        .map_err(api)?;
        let _ = unsafe { CloseHandle(info.hThread) };
        Ok(Launched {
            pid: info.dwProcessId,
            process: ours(info.hProcess),
        })
    }

    fn open_process(&self, pid: u32) -> ApiResult<Handle> {
        unsafe {
            OpenProcess(
                PROCESS_SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            )
        }
        .map(ours)
        .map_err(api)
    }

    fn wait_process(&self, process: Handle) -> ApiResult<u32> {
        unsafe { WaitForSingleObject(raw(process), INFINITE) };
        let mut code = 0;
        unsafe { GetExitCodeProcess(raw(process), &mut code) }.map_err(api)?;
        Ok(code)
    }

    fn terminate(&self, pid: u32) -> ApiResult<()> {
        let process = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }.map_err(api)?;
        let ended = unsafe { TerminateProcess(process, 1) };
        let _ = unsafe { CloseHandle(process) };
        ended.map_err(api)
    }

    fn close(&self, handle: Handle) {
        let _ = unsafe { CloseHandle(raw(handle)) };
    }

    fn create_pipe(
        &self,
        name: &str,
        sddl: &str,
        open_mode: u32,
        pipe_mode: u32,
    ) -> ApiResult<Handle> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(api)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: false.into(),
        };
        let pipe = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(name),
                FILE_FLAGS_AND_ATTRIBUTES(open_mode),
                NAMED_PIPE_MODE(pipe_mode),
                1,
                4096,
                4096,
                0,
                Some(&attributes),
            )
        };
        let failed = windows::core::Error::from_thread();
        unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
        match pipe.is_invalid() {
            true => Err(api(failed)),
            false => Ok(ours(pipe)),
        }
    }

    fn connect_pipe(&self, pipe: Handle) -> ApiResult<()> {
        overlapped(raw(pipe), |ov| unsafe {
            ConnectNamedPipe(raw(pipe), Some(ov))
        })
        .map(|_| ())
    }

    fn pipe_client_pid(&self, pipe: Handle) -> ApiResult<u32> {
        let mut pid = 0;
        unsafe { GetNamedPipeClientProcessId(raw(pipe), &mut pid) }.map_err(api)?;
        Ok(pid)
    }

    fn disconnect_pipe(&self, pipe: Handle) -> ApiResult<()> {
        unsafe { DisconnectNamedPipe(raw(pipe)) }.map_err(api)
    }

    fn open_pipe_client(&self, name: &str) -> ApiResult<Handle> {
        unsafe {
            CreateFileW(
                &HSTRING::from(name),
                GENERIC_READ.0 | GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                None,
            )
        }
        .map(ours)
        .map_err(api)
    }

    fn read_pipe(&self, pipe: Handle, buf: &mut [u8]) -> ApiResult<usize> {
        overlapped(raw(pipe), |ov| unsafe {
            ReadFile(raw(pipe), Some(buf), None, Some(ov))
        })
        .map(|n| n as usize)
    }

    fn write_pipe(&self, pipe: Handle, bytes: &[u8]) -> ApiResult<()> {
        overlapped(raw(pipe), |ov| unsafe {
            WriteFile(raw(pipe), Some(bytes), None, Some(ov))
        })
        .map(|_| ())
    }

    fn protect_dir(&self, path: &Path, sddl: &str) -> ApiResult<()> {
        std::fs::create_dir_all(path).map_err(|e| ApiError {
            code: e.raw_os_error().unwrap_or(0) as u32,
            message: e.to_string(),
        })?;
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .map_err(api)?;
        let (mut present, mut defaulted) = (BOOL(0), BOOL(0));
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let set = unsafe {
            GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
        }
        .map_err(api)
        .and_then(|()| {
            win32(unsafe {
                SetNamedSecurityInfoW(
                    &HSTRING::from(path.as_os_str()),
                    SE_FILE_OBJECT,
                    DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                    None,
                    None,
                    Some(dacl),
                    None,
                )
            })
        });
        unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
        set
    }
    fn send_sas(&self) -> ApiResult<()> {
        unsafe { SendSAS(false) };
        Ok(())
    }

    fn read_dword(&self, key: &str, value: &str) -> ApiResult<u32> {
        let mut data = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        win32(unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                &HSTRING::from(key),
                &HSTRING::from(value),
                RRF_RT_REG_DWORD,
                None,
                Some((&mut data as *mut u32).cast()),
                Some(&mut size),
            )
        })?;
        Ok(data)
    }

    fn write_dword(&self, key: &str, value: &str, data: u32) -> ApiResult<()> {
        win32(unsafe {
            RegSetKeyValueW(
                HKEY_LOCAL_MACHINE,
                &HSTRING::from(key),
                &HSTRING::from(value),
                REG_DWORD.0,
                Some((&data as *const u32).cast()),
                std::mem::size_of::<u32>() as u32,
            )
        })
    }

    fn delete_value(&self, key: &str, value: &str) -> ApiResult<()> {
        win32(unsafe {
            RegDeleteKeyValueW(
                HKEY_LOCAL_MACHINE,
                &HSTRING::from(key),
                &HSTRING::from(value),
            )
        })
    }

    fn create_service(&self, spec: &ServiceSpec) -> ApiResult<()> {
        let manager = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
        )
        .map_err(scm)?;
        let info = ServiceInfo {
            name: spec.name.clone().into(),
            display_name: spec.display_name.clone().into(),
            service_type: ServiceType::OWN_PROCESS,
            start_type: match spec.start {
                StartType::Automatic => ServiceStartType::AutoStart,
            },
            error_control: ServiceErrorControl::Normal,
            executable_path: spec.executable.clone(),
            launch_arguments: vec![],
            dependencies: vec![],
            account_name: match spec.account {
                Account::LocalSystem => None,
            },
            account_password: None,
        };
        manager
            .create_service(&info, ServiceAccess::QUERY_STATUS)
            .map(drop)
            .map_err(scm)
    }

    fn start_service(&self, name: &str) -> ApiResult<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(scm)?;
        let service = manager
            .open_service(name, ServiceAccess::START)
            .map_err(scm)?;
        service.start::<&str>(&[]).map_err(scm)
    }

    fn stop_service(&self, name: &str) -> ApiResult<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(scm)?;
        let service = manager
            .open_service(name, ServiceAccess::STOP)
            .map_err(scm)?;
        service.stop().map(drop).map_err(scm)
    }

    fn delete_service(&self, name: &str) -> ApiResult<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(scm)?;
        let service = manager
            .open_service(name, ServiceAccess::DELETE)
            .map_err(scm)?;
        service.delete().map_err(scm)
    }

    fn add_firewall_rule(&self, rule: &FirewallRule) -> ApiResult<()> {
        netsh_ok(&[
            "add".into(),
            "rule".into(),
            format!("name={}", rule.name),
            "dir=in".into(),
            "action=allow".into(),
            "protocol=UDP".into(),
            format!("localport={}", rule.udp_port),
            format!("program={}", rule.program.display()),
        ])
    }

    fn has_firewall_rule(&self, name: &str) -> ApiResult<bool> {
        netsh(&["show".into(), "rule".into(), format!("name={name}")])
    }

    fn delete_firewall_rule(&self, name: &str) -> ApiResult<()> {
        netsh_ok(&["delete".into(), "rule".into(), format!("name={name}")])
    }
}

/// What `MouserService` runs once the SCM has started it.
static STARTED: Mutex<Option<Started>> = Mutex::new(None);

define_windows_service!(ffi_service_main, service_main);

/// `StartServiceCtrlDispatcherW`: runs `started` as `MouserService` until it
/// stops; an error when the SCM did not start this process.
pub fn run_under_scm(started: Started) -> ApiResult<()> {
    *STARTED.lock().unwrap() = Some(started);
    service_dispatcher::start(SERVICE_NAME, ffi_service_main).map_err(scm)
}

fn service_main(_arguments: Vec<OsString>) {
    let (exits, events) = channel();
    let controls = exits.clone();
    let handler = move |control| match dispatch(control_of(control), &controls) {
        true => ServiceControlHandlerResult::NoError,
        false => ServiceControlHandlerResult::NotImplemented,
    };
    let Ok(status) = service_control_handler::register(SERVICE_NAME, handler) else {
        return;
    };
    let started = STARTED
        .lock()
        .unwrap()
        .take()
        .expect("the service starts once");
    started(&Status(status), exits, events);
}

fn control_of(control: ServiceControl) -> Control {
    match control {
        ServiceControl::Stop => Control::Stop,
        ServiceControl::Preshutdown => Control::Preshutdown,
        ServiceControl::Interrogate => Control::Interrogate,
        ServiceControl::SessionChange(change) => Control::SessionChange(match change.reason {
            SessionChangeReason::ConsoleConnect => SessionReason::ConsoleConnect,
            SessionChangeReason::ConsoleDisconnect => SessionReason::ConsoleDisconnect,
            SessionChangeReason::SessionLogon => SessionReason::Logon,
            SessionChangeReason::SessionLogoff => SessionReason::Logoff,
            _ => SessionReason::Other,
        }),
        _ => Control::Other,
    }
}

struct Status(ServiceStatusHandle);

impl Scm for Status {
    /// `SetServiceStatus`.
    fn report(&self, state: Reported) {
        let (current_state, controls_accepted) = match state {
            Reported::Running => (
                ServiceState::Running,
                ServiceControlAccept::STOP
                    | ServiceControlAccept::PRESHUTDOWN
                    | ServiceControlAccept::SESSION_CHANGE,
            ),
            Reported::Stopped => (ServiceState::Stopped, ServiceControlAccept::empty()),
        };
        let _ = self.0.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state,
            controls_accepted,
            exit_code: ServiceExitCode::Win32(0),
            checkpoint: 0,
            wait_hint: Duration::default(),
            process_id: None,
        });
    }
}
