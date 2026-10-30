//! The service's state machine (contracts/service-ipc.md "Lifecycle"): one
//! session engine in the console session, started again when it exits or
//! the console session changes, and shut down when the service stops. It
//! moves only on events: no polling, sleep or timer, and no back-off.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use mouser_core::logging::{Log, events};

use crate::launch::{Answer, Engine, launch};
use crate::winapi::WinApi;

/// `WTSGetActiveConsoleSessionId` while no session is attached to the console.
const NO_SESSION: u32 = 0xFFFF_FFFF;

/// What moves the state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    /// `SERVICE_CONTROL_SESSIONCHANGE` for the console: connect, disconnect,
    /// logon or logoff.
    SessionChange,
    /// Engine `pid` has ended.
    EngineExited { pid: u32 },
    /// `SERVICE_CONTROL_STOP` or `SERVICE_CONTROL_PRESHUTDOWN`.
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Starting,
    WaitingForSession,
    Running { session: u32, pid: u32 },
    Relaunching,
    Stopping,
    Stopped,
}

pub struct Supervisor {
    api: Arc<dyn WinApi>,
    log: Arc<Log>,
    /// The folder `mouser-service.exe` is in.
    dir: PathBuf,
    sas: Answer,
    /// Where engines' ends are heard.
    exits: Sender<Event>,
    engine: Option<Engine>,
    states: Vec<State>,
}

impl Supervisor {
    pub fn new(
        api: Arc<dyn WinApi>,
        log: Arc<Log>,
        dir: PathBuf,
        sas: Answer,
        exits: Sender<Event>,
    ) -> Self {
        Self {
            api,
            log,
            dir,
            sas,
            exits,
            engine: None,
            states: Vec::new(),
        }
    }

    /// Where it is now.
    pub fn state(&self) -> State {
        *self.states.last().unwrap_or(&State::Starting)
    }

    /// Every state it has been in, in order.
    pub fn states(&self) -> Vec<State> {
        self.states.clone()
    }

    fn enter(&mut self, state: State) {
        if self.states.last() != Some(&state) {
            self.states.push(state);
        }
    }

    /// The service has started: run the engine in the console session, or
    /// wait for one.
    pub fn start(&mut self) {
        self.enter(State::Starting);
        self.find_session();
    }

    /// Runs until stopped: [`start`](Self::start), then each event. Events
    /// that end stop it too.
    pub fn run(&mut self, events: Receiver<Event>) {
        self.start();
        for event in events {
            self.on(event);
            if self.state() == State::Stopped {
                return;
            }
        }
        self.on(Event::Stop);
    }

    pub fn on(&mut self, event: Event) {
        match (event, self.state()) {
            (Event::SessionChange, State::WaitingForSession) => self.find_session(),
            (Event::SessionChange, State::Running { .. }) => {
                self.enter(State::Relaunching);
                self.end_engine();
                self.find_session();
            }
            (
                Event::EngineExited { pid },
                State::Running {
                    session,
                    pid: running,
                },
            ) if pid == running => {
                self.end_engine();
                self.enter(State::Relaunching);
                self.launch_in(session);
            }
            (Event::Stop, State::Running { .. }) => {
                self.enter(State::Stopping);
                self.end_engine();
                self.enter(State::Stopped);
            }
            (Event::Stop, _) => self.enter(State::Stopped),
            _ => {}
        }
    }

    fn find_session(&mut self) {
        let session = self.api.console_session();
        if session == NO_SESSION {
            self.enter(State::WaitingForSession);
            return;
        }
        self.log.write(events::service_session(session));
        self.launch_in(session);
    }

    fn launch_in(&mut self, session: u32) {
        match launch(
            &self.api,
            session,
            &self.dir,
            self.sas.clone(),
            self.exits.clone(),
        ) {
            Ok(engine) => {
                let pid = engine.pid;
                self.log.write(events::service_engine_started(pid, session));
                self.engine = Some(engine);
                self.enter(State::Running { session, pid });
            }
            Err(_) => self.enter(State::WaitingForSession),
        }
    }

    /// Shuts the engine down (it may have ended already) and waits for it.
    fn end_engine(&mut self) {
        let engine = self.engine.take().expect("a running engine");
        engine.shutdown();
        let code = engine.finish();
        self.log.write(events::service_engine_exited(code));
    }
}
