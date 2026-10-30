use std::path::PathBuf;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};

use mouser_core::engine_headless::{FileSecrets, SecretStore};

use crate::args::Entries;
use crate::fake::{FakeWin, NO_SESSION};
use crate::machine::{Machine, Started};
use crate::service::{Reported, Scm};
use crate::session::Event;
use crate::test_dir::TestDir;
use crate::winapi::{ApiError, WinApi};

fn files(dir: PathBuf) -> Box<dyn SecretStore> {
    Box::new(FileSecrets::new(dir))
}

#[derive(Default)]
struct Reports(Mutex<Vec<Reported>>);

impl Scm for Reports {
    fn report(&self, state: Reported) {
        self.0.lock().unwrap().push(state);
    }
}

fn machine(fake: &Arc<FakeWin>, folder: &TestDir) -> Machine {
    Machine {
        api: fake.clone() as Arc<dyn WinApi>,
        dir: folder.path().join("Program Files"),
        machine: folder.path().join("ProgramData"),
        user: folder.path().join("AppData"),
        me: 77,
        host_name: "HOST".into(),
        store: files,
        scm: Box::new(|_| Ok(())),
    }
}

#[test]
fn install_and_uninstall_run_on_this_computer() {
    let fake = Arc::new(FakeWin::new());
    let folder = TestDir::new();
    let machine = machine(&fake, &folder);
    // The installing user never ran Mouser: nothing to copy, no failure.
    assert_eq!(machine.install(), 0);
    let (spec, running) = fake.service("MouserService").unwrap();
    assert_eq!(
        spec.executable,
        folder
            .path()
            .join("Program Files")
            .join("mouser-service.exe")
    );
    assert!(running);
    assert_eq!(machine.uninstall(), 0);
    assert_eq!(fake.service("MouserService"), None);
}

#[test]
fn install_moves_the_installing_users_desk_membership() {
    let fake = Arc::new(FakeWin::new());
    let folder = TestDir::new();
    let machine = machine(&fake, &folder);
    files(machine.machine.join("secrets"))
        .set("device_key", b"key")
        .unwrap();
    std::fs::create_dir_all(&machine.user).unwrap();
    mouser_core::state::LocalState::default()
        .save(&machine.user.join("state.json"))
        .unwrap();
    assert_eq!(machine.install(), 0);
    assert!(machine.machine.join("state.json").exists());
}

#[test]
fn as_the_service_it_runs_the_supervisor_with_its_log_until_stopped() {
    let fake = Arc::new(FakeWin::new());
    fake.set_console(NO_SESSION);
    let folder = TestDir::new();
    let reports = Arc::new(Reports::default());
    let mut machine = machine(&fake, &folder);
    std::fs::create_dir_all(&machine.machine).unwrap();
    let mut config = mouser_core::config::load_or_create(&machine.machine);
    config.diagnostics.logging = true;
    mouser_core::config::save(&machine.machine, &config);
    machine.scm = Box::new({
        let reports = reports.clone();
        move |started: Started| {
            let (events, heard) = channel();
            events.send(Event::Stop).unwrap();
            started(&*reports, events, heard);
            Ok(())
        }
    });
    assert_eq!(machine.service(), 0);
    assert_eq!(
        *reports.0.lock().unwrap(),
        [Reported::Running, Reported::Stopped]
    );
    // Its identity is the machine store's, and its log the machine folder's.
    assert!(machine.machine.join("secrets").join("device_key").exists());
    assert!(machine.machine.join("logs").join("service.log").exists());
}

#[test]
fn a_service_the_scm_does_not_start_exits_1() {
    let fake = Arc::new(FakeWin::new());
    let folder = TestDir::new();
    let mut machine = machine(&fake, &folder);
    machine.scm = Box::new(|_| {
        Err(ApiError {
            code: 1063,
            message: "not started by the SCM".into(),
        })
    });
    assert_eq!(machine.service(), 1);
}
