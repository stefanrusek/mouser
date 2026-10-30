//! A stand-in for Windows behind [`WinApi`] (OD-5), for the unit and
//! acceptance tests: sessions, processes, the engine's pipe and the machine's
//! registry, services and firewall live in memory, every call is recorded,
//! and the test plays the session engine (connect, send, exit).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard};

use mouser_proto::service_ipc::{EngineMessage, ServiceMessage};
use mouser_proto::wire::{decode_frame, encode_frame};

use crate::winapi::{
    ApiError, ApiResult, ERROR_FILE_NOT_FOUND, ERROR_SERVICE_DOES_NOT_EXIST,
    ERROR_SERVICE_NOT_ACTIVE, FirewallRule, Handle, Launched, ProcessEntry, ServiceSpec, WinApi,
};

/// No console session (`WTSGetActiveConsoleSessionId`).
pub const NO_SESSION: u32 = 0xFFFF_FFFF;
/// `ERROR_ACCESS_DENIED`: a second instance of a first-instance pipe.
pub const ERROR_ACCESS_DENIED: u32 = 5;
/// `ERROR_BROKEN_PIPE`: the client has gone.
pub const ERROR_BROKEN_PIPE: u32 = 109;
/// `ERROR_PIPE_BUSY`: the pipe has a client already.
pub const ERROR_PIPE_BUSY: u32 = 231;
/// The process id the service itself has.
pub const SERVICE_PID: u32 = 4;

/// One call the service made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    CreatePipe {
        name: String,
        sddl: String,
        open_mode: u32,
        pipe_mode: u32,
    },
    DuplicateToken(u32),
    CreateProcess {
        token: Handle,
        application: PathBuf,
        command_line: String,
        desktop: String,
    },
    Disconnect(Handle),
    Close(Handle),
    Terminate(u32),
    ProtectDir(PathBuf, String),
    SendSas,
    ReadDword(String, String),
    WriteDword(String, String, u32),
    DeleteValue(String, String),
    CreateService(ServiceSpec),
    StartService(String),
    StopService(String),
    DeleteService(String),
    AddFirewallRule(FirewallRule),
    DeleteFirewallRule(String),
}

#[derive(Default)]
struct Pipe {
    name: String,
    /// The client connected now, by process id.
    client: Option<u32>,
    /// Bytes the client sent and the service has not read.
    inbox: VecDeque<u8>,
    /// Bytes the service wrote.
    outbox: Vec<u8>,
    /// The client has gone since it connected.
    gone: bool,
    closed: bool,
}

struct Process {
    entry: ProcessEntry,
    exit: Option<u32>,
}

#[derive(Default)]
struct State {
    console: u32,
    processes: Vec<Process>,
    next_pid: u32,
    next_handle: isize,
    /// Handles to processes, by handle.
    process_handles: HashMap<Handle, u32>,
    /// The session each duplicated token came from.
    tokens: HashMap<Handle, u32>,
    pipes: HashMap<Handle, Pipe>,
    registry: HashMap<(String, String), u32>,
    services: HashMap<String, (ServiceSpec, bool)>,
    rules: Vec<FirewallRule>,
    /// What the next calls of a method do: fail, or (`None`) succeed.
    failures: HashMap<&'static str, VecDeque<Option<ApiError>>>,
    calls: Vec<Call>,
    /// A stopped service's process goes on running.
    slow_stop: bool,
}

pub struct FakeWin {
    state: Mutex<State>,
    changed: Condvar,
}

impl Default for FakeWin {
    fn default() -> Self {
        Self::new()
    }
}

fn error(code: u32, message: &str) -> ApiError {
    ApiError {
        code,
        message: message.into(),
    }
}

impl FakeWin {
    /// A computer whose console session is 1, with its `winlogon.exe`.
    pub fn new() -> Self {
        let fake = Self {
            state: Mutex::new(State {
                console: 1,
                next_pid: 1000,
                next_handle: 1,
                ..State::default()
            }),
            changed: Condvar::new(),
        };
        fake.start_process("winlogon.exe", 1);
        fake
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    fn wait_while(&self, mut until: impl FnMut(&mut State) -> bool) -> MutexGuard<'_, State> {
        self.changed
            .wait_while(self.state(), |s| !until(s))
            .unwrap()
    }

    fn update<T>(&self, change: impl FnOnce(&mut State) -> T) -> T {
        let out = change(&mut self.state());
        self.changed.notify_all();
        out
    }

    fn take_failure(&self, method: &'static str) -> ApiResult<()> {
        match self
            .state()
            .failures
            .get_mut(method)
            .and_then(VecDeque::pop_front)
            .flatten()
        {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn handle(state: &mut State) -> Handle {
        state.next_handle += 1;
        Handle(state.next_handle)
    }

    /// The next call to `method` (the trait method's name) fails with `code`.
    pub fn fail(&self, method: &'static str, code: u32) {
        self.update(|s| {
            s.failures
                .entry(method)
                .or_default()
                .push_back(Some(error(code, &format!("{method} failed"))))
        });
    }

    /// The next call to `method` succeeds; one queued with [`fail`](Self::fail)
    /// after it fails the call after that.
    pub fn pass(&self, method: &'static str) {
        self.update(|s| s.failures.entry(method).or_default().push_back(None));
    }
    /// Every call so far, in order.
    pub fn calls(&self) -> Vec<Call> {
        self.state().calls.clone()
    }

    fn call(&self, call: Call) {
        self.update(|s| s.calls.push(call));
    }

    /// The console session becomes `session` ([`NO_SESSION`] for none).
    pub fn set_console(&self, session: u32) {
        self.update(|s| s.console = session);
    }

    /// A process named `name` starts in `session`; its id.
    pub fn start_process(&self, name: &str, session: u32) -> u32 {
        self.update(|s| {
            s.next_pid += 1;
            let pid = s.next_pid;
            s.processes.push(Process {
                entry: ProcessEntry {
                    pid,
                    session,
                    name: name.into(),
                },
                exit: None,
            });
            pid
        })
    }

    /// The session process `pid` runs in.
    pub fn session_of(&self, pid: u32) -> Option<u32> {
        self.state()
            .processes
            .iter()
            .find(|p| p.entry.pid == pid)
            .map(|p| p.entry.session)
    }
    /// The processes still running, as `(name, session)`.
    pub fn running(&self) -> Vec<(String, u32)> {
        self.state()
            .processes
            .iter()
            .filter(|p| p.exit.is_none())
            .map(|p| (p.entry.name.clone(), p.entry.session))
            .collect()
    }

    /// A service told to stop is still ending when the SCM returns: its
    /// process ends when the test says.
    pub fn slow_to_stop(&self) {
        self.update(|s| s.slow_stop = true);
    }

    /// Process `pid` ends with `code`; a pipe it was connected to sees it go.
    pub fn exit(&self, pid: u32, code: u32) {
        self.update(|s| exit(s, pid, code));
    }

    /// The pipe named `name` that is open now.
    fn open_pipe<'a>(state: &'a mut State, name: &str) -> Option<(&'a Handle, &'a mut Pipe)> {
        state
            .pipes
            .iter_mut()
            .find(|(_, p)| p.name == name && !p.closed)
    }

    /// Whether a pipe named `name` is open.
    pub fn has_pipe(&self, name: &str) -> bool {
        Self::open_pipe(&mut self.state(), name).is_some()
    }

    /// Process `pid` connects to the open pipe `name`, waiting until it exists.
    pub fn connect(&self, name: &str, pid: u32) {
        let mut state = self.wait_while(|s| Self::open_pipe(s, name).is_some());
        let (_, pipe) = Self::open_pipe(&mut state, name).unwrap();
        pipe.client = Some(pid);
        pipe.gone = false;
        drop(state);
        self.changed.notify_all();
    }

    /// The session engine the service starts next in `session` connects to
    /// its pipe, as it does once it has started; its process id.
    pub fn connect_engine(&self, session: u32) -> u32 {
        let name = mouser_proto::service_ipc::engine_pipe_name(session);
        let unconnected = |s: &State| {
            s.processes
                .iter()
                .filter(|p| p.exit.is_none() && p.entry.session == session)
                .filter(|p| p.entry.name == "mouser-app.exe")
                .map(|p| p.entry.pid)
                .find(|pid| s.pipes.values().all(|pipe| pipe.client != Some(*pid)))
        };
        let state = self.wait_while(|s| {
            let waiting = s
                .pipes
                .values()
                .any(|p| p.name == name && !p.closed && p.client.is_none());
            waiting && unconnected(s).is_some()
        });
        let pid = unconnected(&state).unwrap();
        drop(state);
        self.connect(&name, pid);
        pid
    }

    /// The client of pipe `name` sends `bytes`.
    pub fn send_bytes(&self, name: &str, bytes: &[u8]) {
        self.update(|s| {
            let (_, pipe) = Self::open_pipe(s, name).expect("the pipe is open");
            pipe.inbox.extend(bytes);
        });
    }

    /// The client of pipe `name` sends `msg`.
    pub fn send(&self, name: &str, msg: &EngineMessage) {
        self.send_bytes(name, &encode_frame(msg));
    }

    /// Everything the service wrote to pipe `name` so far.
    pub fn received(&self, name: &str) -> Vec<ServiceMessage> {
        let state = self.state();
        let mut bytes: Vec<u8> = state
            .pipes
            .values()
            .filter(|p| p.name == name)
            .flat_map(|p| p.outbox.clone())
            .collect();
        let mut out = Vec::new();
        while !bytes.is_empty() {
            let (msg, used) = decode_frame(&bytes).expect("whole frames");
            out.push(msg);
            bytes.drain(..used);
        }
        out
    }

    /// Waits until the service has written `n` messages to pipe `name`.
    pub fn await_received(&self, name: &str, n: usize) -> Vec<ServiceMessage> {
        drop(self.wait_while(|s| {
            let bytes: Vec<u8> = s
                .pipes
                .values()
                .filter(|p| p.name == name)
                .flat_map(|p| p.outbox.clone())
                .collect();
            let mut count = 0;
            let mut at = 0;
            while let Ok((_, used)) = decode_frame::<ServiceMessage>(&bytes[at..]) {
                at += used;
                count += 1;
            }
            count >= n
        }));
        self.received(name)
    }

    /// Waits until `seen` holds of the calls made so far.
    pub fn await_calls(&self, mut seen: impl FnMut(&[Call]) -> bool) -> Vec<Call> {
        self.wait_while(|s| seen(&s.calls)).calls.clone()
    }

    /// The registry value `key\value` under HKLM.
    pub fn dword(&self, key: &str, value: &str) -> Option<u32> {
        self.state()
            .registry
            .get(&(key.to_string(), value.to_string()))
            .copied()
    }

    /// Sets the registry value `key\value` under HKLM.
    pub fn set_dword(&self, key: &str, value: &str, data: u32) {
        self.update(|s| s.registry.insert((key.into(), value.into()), data));
    }

    /// The service named `name`, and whether it runs.
    pub fn service(&self, name: &str) -> Option<(ServiceSpec, bool)> {
        self.state().services.get(name).cloned()
    }

    /// The firewall rules.
    pub fn rules(&self) -> Vec<FirewallRule> {
        self.state().rules.clone()
    }
}

fn exit(state: &mut State, pid: u32, code: u32) {
    let process = state.processes.iter_mut().find(|p| p.entry.pid == pid);
    process
        .expect("a process this computer has")
        .exit
        .get_or_insert(code);
    for pipe in state.pipes.values_mut() {
        if pipe.client == Some(pid) {
            pipe.gone = true;
        }
    }
}

impl WinApi for FakeWin {
    fn console_session(&self) -> u32 {
        self.state().console
    }

    fn processes(&self) -> ApiResult<Vec<ProcessEntry>> {
        self.take_failure("processes")?;
        Ok(self
            .state()
            .processes
            .iter()
            .filter(|p| p.exit.is_none())
            .map(|p| p.entry.clone())
            .collect())
    }

    fn duplicate_token(&self, pid: u32) -> ApiResult<Handle> {
        self.call(Call::DuplicateToken(pid));
        self.take_failure("duplicate_token")?;
        Ok(self.update(|s| {
            let session = s
                .processes
                .iter()
                .find(|p| p.entry.pid == pid)
                .map_or(NO_SESSION, |p| p.entry.session);
            let token = Self::handle(s);
            s.tokens.insert(token, session);
            token
        }))
    }

    fn create_process_as_user(
        &self,
        token: Handle,
        application: &Path,
        command_line: &str,
        desktop: &str,
    ) -> ApiResult<Launched> {
        self.call(Call::CreateProcess {
            token,
            application: application.into(),
            command_line: command_line.into(),
            desktop: desktop.into(),
        });
        self.take_failure("create_process_as_user")?;
        let session = self.state().tokens[&token];
        let name = application.file_name().unwrap().to_string_lossy();
        let pid = self.start_process(&name, session);
        Ok(self.update(|s| {
            let process = Self::handle(s);
            s.process_handles.insert(process, pid);
            Launched { pid, process }
        }))
    }

    fn open_process(&self, pid: u32) -> ApiResult<Handle> {
        self.take_failure("open_process")?;
        Ok(self.update(|s| {
            let process = Self::handle(s);
            s.process_handles.insert(process, pid);
            process
        }))
    }

    fn wait_process(&self, process: Handle) -> ApiResult<u32> {
        self.take_failure("wait_process")?;
        let pid = self.state().process_handles[&process];
        let state = self.wait_while(|s| {
            s.processes
                .iter()
                .any(|p| p.entry.pid == pid && p.exit.is_some())
        });
        Ok(state
            .processes
            .iter()
            .find(|p| p.entry.pid == pid)
            .and_then(|p| p.exit)
            .unwrap())
    }

    fn terminate(&self, pid: u32) -> ApiResult<()> {
        self.call(Call::Terminate(pid));
        self.take_failure("terminate")?;
        self.exit(pid, 1);
        Ok(())
    }

    fn close(&self, handle: Handle) {
        self.call(Call::Close(handle));
        self.update(|s| {
            if let Some(pipe) = s.pipes.get_mut(&handle) {
                pipe.closed = true;
            }
        });
    }

    fn create_pipe(
        &self,
        name: &str,
        sddl: &str,
        open_mode: u32,
        pipe_mode: u32,
    ) -> ApiResult<Handle> {
        self.call(Call::CreatePipe {
            name: name.into(),
            sddl: sddl.into(),
            open_mode,
            pipe_mode,
        });
        self.take_failure("create_pipe")?;
        self.update(|s| {
            if Self::open_pipe(s, name).is_some() {
                return Err(error(ERROR_ACCESS_DENIED, "the pipe exists"));
            }
            let handle = Self::handle(s);
            s.pipes.insert(
                handle,
                Pipe {
                    name: name.into(),
                    ..Pipe::default()
                },
            );
            Ok(handle)
        })
    }

    fn connect_pipe(&self, pipe: Handle) -> ApiResult<()> {
        self.take_failure("connect_pipe")?;
        drop(self.wait_while(|s| s.pipes[&pipe].client.is_some()));
        Ok(())
    }

    fn pipe_client_pid(&self, pipe: Handle) -> ApiResult<u32> {
        self.take_failure("pipe_client_pid")?;
        Ok(self.state().pipes[&pipe].client.unwrap())
    }

    fn disconnect_pipe(&self, pipe: Handle) -> ApiResult<()> {
        let done = self.take_failure("disconnect_pipe");
        // Seen and done at once: a test that waits for it may connect next.
        self.update(|s| {
            s.calls.push(Call::Disconnect(pipe));
            if done.is_ok() {
                let p = s.pipes.get_mut(&pipe).unwrap();
                p.client = None;
                p.inbox.clear();
            }
        });
        done
    }

    fn open_pipe_client(&self, name: &str) -> ApiResult<Handle> {
        self.update(|s| {
            let (_, pipe) = Self::open_pipe(s, name).expect("the pipe is open");
            if pipe.client.is_some() {
                return Err(error(ERROR_PIPE_BUSY, "all pipe instances are busy"));
            }
            pipe.client = Some(SERVICE_PID);
            Ok(Self::handle(s))
        })
    }

    fn read_pipe(&self, pipe: Handle, buf: &mut [u8]) -> ApiResult<usize> {
        let mut state = self.wait_while(|s| {
            let p = &s.pipes[&pipe];
            !p.inbox.is_empty() || p.gone
        });
        let p = state.pipes.get_mut(&pipe).unwrap();
        if p.inbox.is_empty() {
            return Err(error(ERROR_BROKEN_PIPE, "the pipe has been ended"));
        }
        let n = buf.len().min(p.inbox.len());
        for (to, from) in buf.iter_mut().zip(p.inbox.drain(..n)) {
            *to = from;
        }
        Ok(n)
    }

    fn write_pipe(&self, pipe: Handle, bytes: &[u8]) -> ApiResult<()> {
        self.take_failure("write_pipe")?;
        self.update(|s| {
            let p = s.pipes.get_mut(&pipe).unwrap();
            p.outbox.extend(bytes);
            let shutdown = encode_frame(&ServiceMessage::Shutdown);
            // An engine told to shut down does, with 0.
            if bytes == shutdown.as_slice()
                && let Some(client) = s.pipes[&pipe].client
            {
                exit(s, client, 0);
            }
        });
        Ok(())
    }

    fn protect_dir(&self, path: &Path, sddl: &str) -> ApiResult<()> {
        self.call(Call::ProtectDir(path.into(), sddl.into()));
        self.take_failure("protect_dir")
    }
    fn send_sas(&self) -> ApiResult<()> {
        self.call(Call::SendSas);
        self.take_failure("send_sas")
    }

    fn read_dword(&self, key: &str, value: &str) -> ApiResult<u32> {
        self.call(Call::ReadDword(key.into(), value.into()));
        self.take_failure("read_dword")?;
        self.dword(key, value)
            .ok_or_else(|| error(ERROR_FILE_NOT_FOUND, "not found"))
    }

    fn write_dword(&self, key: &str, value: &str, data: u32) -> ApiResult<()> {
        self.call(Call::WriteDword(key.into(), value.into(), data));
        self.take_failure("write_dword")?;
        self.set_dword(key, value, data);
        Ok(())
    }

    fn delete_value(&self, key: &str, value: &str) -> ApiResult<()> {
        self.call(Call::DeleteValue(key.into(), value.into()));
        self.take_failure("delete_value")?;
        self.update(|s| s.registry.remove(&(key.to_string(), value.to_string())))
            .map(drop)
            .ok_or_else(|| error(ERROR_FILE_NOT_FOUND, "not found"))
    }

    fn create_service(&self, spec: &ServiceSpec) -> ApiResult<()> {
        self.call(Call::CreateService(spec.clone()));
        self.take_failure("create_service")?;
        self.update(|s| s.services.insert(spec.name.clone(), (spec.clone(), false)));
        Ok(())
    }

    fn start_service(&self, name: &str) -> ApiResult<()> {
        self.call(Call::StartService(name.into()));
        self.take_failure("start_service")?;
        // The service was created first: install is the only caller.
        self.update(|s| s.services.get_mut(name).unwrap().1 = true);
        Ok(())
    }

    fn stop_service(&self, name: &str) -> ApiResult<()> {
        self.call(Call::StopService(name.into()));
        self.take_failure("stop_service")?;
        self.update(|s| match s.services.get_mut(name) {
            Some((spec, running)) if *running => {
                *running = false;
                let image = spec
                    .executable
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                let ending: Vec<u32> = s
                    .processes
                    .iter()
                    .filter(|p| p.entry.session == 0 && p.entry.name == image)
                    .map(|p| p.entry.pid)
                    .collect();
                if !s.slow_stop {
                    for pid in ending {
                        exit(s, pid, 0);
                    }
                }
                Ok(())
            }
            Some(_) => Err(error(ERROR_SERVICE_NOT_ACTIVE, "not running")),
            None => Err(error(ERROR_SERVICE_DOES_NOT_EXIST, "no such service")),
        })
    }

    fn delete_service(&self, name: &str) -> ApiResult<()> {
        self.call(Call::DeleteService(name.into()));
        self.take_failure("delete_service")?;
        self.update(|s| s.services.remove(name))
            .map(drop)
            .ok_or_else(|| error(ERROR_SERVICE_DOES_NOT_EXIST, "no such service"))
    }

    fn add_firewall_rule(&self, rule: &FirewallRule) -> ApiResult<()> {
        self.call(Call::AddFirewallRule(rule.clone()));
        self.take_failure("add_firewall_rule")?;
        self.update(|s| s.rules.push(rule.clone()));
        Ok(())
    }

    fn has_firewall_rule(&self, name: &str) -> ApiResult<bool> {
        self.take_failure("has_firewall_rule")?;
        Ok(self.state().rules.iter().any(|r| r.name == name))
    }

    fn delete_firewall_rule(&self, name: &str) -> ApiResult<()> {
        self.call(Call::DeleteFirewallRule(name.into()));
        self.take_failure("delete_firewall_rule")?;
        self.update(|s| s.rules.retain(|r| r.name != name));
        Ok(())
    }
}
