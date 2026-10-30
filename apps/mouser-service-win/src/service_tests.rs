use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use mouser_proto::service_ipc::SasResult;

use crate::fake::{FakeWin, NO_SESSION};
use crate::service::{Control, Reported, Scm, SessionReason, dispatch, serve};
use crate::session::{Event, Supervisor};
use crate::test_log::TestLog;
use crate::winapi::WinApi;

#[test]
fn stop_and_shutdown_stop_the_supervisor() {
    for control in [Control::Stop, Control::Preshutdown] {
        let (events, heard) = channel();
        assert!(dispatch(control, &events));
        assert_eq!(heard.try_recv(), Ok(Event::Stop));
    }
}

#[test]
fn console_sessions_coming_and_going_are_session_changes_and_locking_is_not() {
    for reason in [
        SessionReason::ConsoleConnect,
        SessionReason::ConsoleDisconnect,
        SessionReason::Logon,
        SessionReason::Logoff,
    ] {
        let (events, heard) = channel();
        assert!(dispatch(Control::SessionChange(reason), &events));
        assert_eq!(heard.try_recv(), Ok(Event::SessionChange), "{reason:?}");
    }
    let (events, heard) = channel();
    assert!(dispatch(
        Control::SessionChange(SessionReason::Other),
        &events
    ));
    assert!(heard.try_recv().is_err());
}

#[test]
fn the_scm_asking_after_it_is_answered_and_anything_else_is_not_implemented() {
    let (events, heard) = channel();
    assert!(dispatch(Control::Interrogate, &events));
    assert!(!dispatch(Control::Other, &events));
    assert!(heard.try_recv().is_err());
}

#[derive(Default)]
struct Reports(Mutex<Vec<Reported>>);

impl Scm for Reports {
    fn report(&self, state: Reported) {
        self.0.lock().unwrap().push(state);
    }
}

#[test]
fn the_service_reports_running_until_its_supervisor_stops_then_stopped() {
    let fake = Arc::new(FakeWin::new());
    fake.set_console(NO_SESSION);
    let log = TestLog::new();
    let (events, heard) = channel();
    let supervisor = Supervisor::new(
        fake as Arc<dyn WinApi>,
        log.log.clone(),
        r"C:\Mouser".into(),
        Arc::new(|| SasResult::Sent),
        events.clone(),
    );
    let scm = Arc::new(Reports::default());
    let serving = std::thread::spawn({
        let scm = scm.clone();
        move || serve(&*scm, supervisor, heard)
    });
    assert!(dispatch(Control::Stop, &events));
    serving.join().unwrap();
    assert_eq!(
        *scm.0.lock().unwrap(),
        [Reported::Running, Reported::Stopped]
    );
}
