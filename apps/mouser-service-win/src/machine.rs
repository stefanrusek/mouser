//! This computer as `mouser-service.exe` finds it: the real Windows behind
//! the seam, the folders, and the SCM. `main` fills it in; tests use the
//! fake.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender};

use mouser_core::engine_headless::SecretStore;
use mouser_core::identity::Identity;

use crate::args::Entries;
use crate::install::{exit_code, install, uninstall};
use crate::migrate::migrate;
use crate::service::{Scm, serve};
use crate::session::{Event, Supervisor};
use crate::winapi::{ApiError, WinApi};

/// What runs once the SCM has started the service.
pub type Started = Box<dyn FnOnce(&dyn Scm, Sender<Event>, Receiver<Event>) + Send>;

pub struct Machine {
    pub api: Arc<dyn WinApi>,
    /// The folder `mouser-service.exe` and `mouser-app.exe` are in.
    pub dir: PathBuf,
    /// `C:\ProgramData\Mouser`.
    pub machine: PathBuf,
    /// The installing user's Mouser folder.
    pub user: PathBuf,
    /// This process.
    pub me: u32,
    /// The computer's host name, for a computer with no name of its own.
    pub host_name: String,
    /// The installed Mouser's store of secrets in a folder.
    pub store: fn(PathBuf) -> Box<dyn SecretStore>,
    /// Runs `Started` as `MouserService` under the SCM.
    pub scm: Box<dyn Fn(Started) -> Result<(), ApiError>>,
}

impl Entries for Machine {
    fn install(&self) -> i32 {
        let store = self.store;
        let secrets = self.machine.join("secrets");
        let migrate = || migrate(&self.user, &self.machine, &|| store(secrets.clone()));
        let done = install(self.api.as_ref(), &self.dir, &self.machine, &migrate);
        exit_code(done, &mut std::io::stderr())
    }

    fn uninstall(&self) -> i32 {
        exit_code(
            uninstall(self.api.as_ref(), self.me),
            &mut std::io::stderr(),
        )
    }

    fn service(&self) -> i32 {
        let (api, dir, machine) = (self.api.clone(), self.dir.clone(), self.machine.clone());
        let (host_name, store) = (self.host_name.clone(), self.store);
        let started = (self.scm)(Box::new(move |scm, exits, events| {
            let id = Identity::load_or_create(&mut *store(machine.join("secrets"))).computer_id();
            let log = Arc::new(crate::log::open(&machine, id, &host_name));
            let sas = crate::sas::answering(api.clone(), log.clone());
            serve(scm, Supervisor::new(api, log, dir, sas, exits), events);
        }));
        let started = started.map_err(|e| format!("running as MouserService: {e}"));
        exit_code(started, &mut std::io::stderr())
    }
}
