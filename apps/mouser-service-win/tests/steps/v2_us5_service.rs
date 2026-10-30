//! Step definitions for `v2_us5_service.feature`. The computer is the fake
//! `WinApi`: its console session, `winlogon.exe`, registry, services and
//! processes. `MouserService` is the real state machine, started the way the
//! SCM starts it, and the session engine is played by the test (it connects
//! to its pipe, asks for Ctrl-Alt-Del, and exits when told to shut down).

use std::fmt;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};
use std::thread::JoinHandle;

use cucumber::{World, given, then, when};
use mouser_core::logging::{Context, Log, SERVICE_FILE};
use mouser_proto::ids::ComputerId;
use mouser_proto::service_ipc::{EngineMessage, SasResult, ServiceMessage, engine_pipe_name};
use mouser_service_win::fake::{Call, FakeWin};
use mouser_service_win::install::{install, uninstall};
use mouser_service_win::sas::{POLICY_KEY, POLICY_VALUE, answering};
use mouser_service_win::service::{Control, SessionReason, dispatch};
use mouser_service_win::session::{Event, Supervisor};
use mouser_service_win::winapi::WinApi;

const DIR: &str = r"C:\Program Files\Mouser";
const MACHINE: &str = r"C:\ProgramData\Mouser";
/// The uninstaller's own process.
const UNINSTALLER: u32 = 9;

#[derive(World)]
#[world(init = Self::new)]
pub struct ServiceWorld {
    fake: Arc<FakeWin>,
    /// The SCM's line to the running service.
    controls: Option<Sender<Event>>,
    service: Option<JoinHandle<()>>,
    /// The `mouser-service.exe` process the SCM started.
    service_pid: u32,
    /// The session engine's console session and process, once connected.
    engine: Option<(u32, u32)>,
}

impl fmt::Debug for ServiceWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceWorld")
            .field("engine", &self.engine)
            .finish()
    }
}

impl ServiceWorld {
    fn new() -> Self {
        Self {
            fake: Arc::new(FakeWin::new()),
            controls: None,
            service: None,
            service_pid: 0,
            engine: None,
        }
    }

    /// Mouser's installer has run and the SCM has started `MouserService`.
    fn installed(&mut self) {
        install(
            self.fake.as_ref(),
            Path::new(DIR),
            Path::new(MACHINE),
            &|| Ok(()),
        )
        .unwrap();
        self.service_pid = self.fake.start_process("mouser-service.exe", 0);
        let api: Arc<dyn WinApi> = self.fake.clone();
        let log = Arc::new(Log::named(
            &std::env::temp_dir(),
            SERVICE_FILE,
            ComputerId([7; 16]),
            "Beta".into(),
            false,
            8,
            Context::fallback(),
        ));
        let sas = answering(api.clone(), log.clone());
        let (events, heard) = channel();
        let mut supervisor = Supervisor::new(api, log, DIR.into(), sas, events.clone());
        self.controls = Some(events);
        self.service = Some(std::thread::spawn(move || supervisor.run(heard)));
    }

    fn control(&self, control: Control) {
        assert!(dispatch(control, self.controls.as_ref().unwrap()));
    }

    /// The session engine of `session` starts and connects to its pipe.
    fn engine_connects(&mut self, session: u32) {
        let pid = self.fake.connect_engine(session);
        self.engine = Some((session, pid));
    }

    fn pipe(&self) -> String {
        engine_pipe_name(self.engine.unwrap().0)
    }

    /// The SCM stops the service, which shuts its engine down first.
    pub fn stop(&mut self) {
        if let Some(service) = self.service.take() {
            self.control(Control::Stop);
            service.join().unwrap();
        }
    }

    /// The last engine started runs in `session` as SYSTEM, with the token
    /// of that session's `winlogon.exe`, on the desktop where sign-in and
    /// prompts are shown.
    fn assert_engine_in(&self, session: u32) {
        let calls = self.fake.calls();
        let at = calls
            .iter()
            .rposition(|c| matches!(c, Call::CreateProcess { .. }))
            .unwrap();
        let Call::DuplicateToken(winlogon) = calls[at - 1] else {
            panic!("{:?}", calls[at - 1])
        };
        assert_eq!(self.fake.session_of(winlogon), Some(session));
        let Call::CreateProcess {
            application,
            command_line,
            desktop,
            ..
        } = &calls[at]
        else {
            unreachable!()
        };
        assert_eq!(application, &Path::new(DIR).join("mouser-app.exe"));
        assert_eq!(
            command_line,
            &format!(r#""{DIR}\mouser-app.exe" --headless --machine"#)
        );
        assert_eq!(desktop, r"winsta0\default");
        assert_eq!(self.engine.unwrap().0, session);
    }
}

#[given("Mouser is installed on a Windows computer that has just started or is locked")]
fn started_or_locked(world: &mut ServiceWorld) {
    // The sign-in screen is the console session's Winlogon desktop.
    world.installed();
    // Locking is no change the service follows: the engine stays.
    world.control(Control::SessionChange(SessionReason::Other));
}

#[given(
    "Mouser is installed on a Windows computer that shows a permission elevation prompt or a user-switching screen"
)]
fn prompt_or_switch(world: &mut ServiceWorld) {
    world.installed();
    world.engine_connects(1);
    // An elevation prompt is a desktop of the same session: no restart.
    world.control(Control::SessionChange(SessionReason::Other));
    // Another user signs in at the switching screen: a new console session.
    world.fake.start_process("winlogon.exe", 2);
    world.fake.set_console(2);
    world.control(Control::SessionChange(SessionReason::ConsoleConnect));
}

#[given("Mouser is installed and the active computer runs Windows")]
fn installed_and_active(world: &mut ServiceWorld) {
    world.installed();
    world.engine_connects(1);
}

#[given("Mouser is installed on a Windows computer")]
fn installed_with_tray(world: &mut ServiceWorld) {
    world.installed();
    world.engine_connects(1);
    // The signed-in user's tray app in UI mode, and a cursor guard.
    world.fake.start_process("mouser-app.exe", 1);
    world.fake.start_process("mouser-app.exe", 1);
}

#[when("the desk user crosses to it")]
fn crosses(world: &mut ServiceWorld) {
    let session = world.fake.console_session();
    world.engine_connects(session);
}

#[then("they can sign in with the shared mouse and keyboard")]
fn can_sign_in(world: &mut ServiceWorld) {
    world.assert_engine_in(1);
}

#[then(
    "they can operate that screen, and after a switch the newly signed-in user's session is controllable"
)]
fn can_operate_after_switch(world: &mut ServiceWorld) {
    world.assert_engine_in(2);
    // The first session's engine was told to shut down, and has.
    assert_eq!(
        world.fake.received(&engine_pipe_name(1)),
        [ServiceMessage::Shutdown]
    );
    let engines: Vec<_> = world
        .fake
        .running()
        .into_iter()
        .filter(|(name, _)| name == "mouser-app.exe")
        .collect();
    assert_eq!(engines, [("mouser-app.exe".to_string(), 2)]);
}

#[when("the desk user chooses Send Ctrl-Alt-Del from the tray menu")]
fn send_ctrl_alt_del(world: &mut ServiceWorld) {
    world.fake.send(&world.pipe(), &EngineMessage::SendSas);
}

#[then(
    "that computer responds as if Ctrl-Alt-Del had been pressed on its own keyboard; where that computer's policy prevents it, its Health entry states so under \"What doesn't work here\""
)]
fn responds(world: &mut ServiceWorld) {
    let pipe = world.pipe();
    assert_eq!(
        world.fake.await_received(&pipe, 1),
        [ServiceMessage::SasResult(SasResult::Sent)]
    );
    assert!(world.fake.calls().contains(&Call::SendSas));
    // Where policy keeps services from sending it, the engine hears so; its
    // platform states the limitation (platform-windows' tests).
    world.fake.set_dword(POLICY_KEY, POLICY_VALUE, 0);
    world.fake.send(&pipe, &EngineMessage::SendSas);
    assert_eq!(
        world.fake.await_received(&pipe, 2)[1],
        ServiceMessage::SasResult(SasResult::PolicyBlocked)
    );
}

#[when("the desk owner uninstalls it")]
fn uninstalls(world: &mut ServiceWorld) {
    // The SCM's stop reaches the service, which ends its engine and then
    // its own process; the uninstaller waits for that.
    world.fake.slow_to_stop();
    let fake = world.fake.clone();
    let uninstalling = std::thread::spawn(move || uninstall(fake.as_ref(), UNINSTALLER));
    world
        .fake
        .await_calls(|calls| calls.contains(&Call::StopService("MouserService".into())));
    world.control(Control::Stop);
    world.service.take().unwrap().join().unwrap();
    world.fake.exit(world.service_pid, 0);
    uninstalling.join().unwrap().unwrap();
}

#[then("no part of Mouser is left running on that computer")]
fn nothing_left(world: &mut ServiceWorld) {
    let left: Vec<_> = world
        .fake
        .running()
        .into_iter()
        .filter(|(name, _)| name.starts_with("mouser"))
        .collect();
    assert_eq!(left, []);
    assert_eq!(world.fake.service("MouserService"), None);
}
