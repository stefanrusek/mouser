//! Step definitions for `features/us7.feature`: files can be dragged from one
//! computer to another.

use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use cucumber::{World, given, then, when};
use mouser_platform::{Capability, Limitation, LocalInput, Os, PlatformInfo, SessionType};
use mouser_proto::control::{ProblemKind, Topic};
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::Value;

use crate::support::desk::{Lab, Setup};

const WAIT: Duration = Duration::from_secs(20);

/// The state shared by the steps of one scenario.
#[derive(World)]
#[world(init = Self::new)]
pub struct DragWorld {
    pub lab: Lab,
    /// Where the dragged files live on Alpha.
    source: Option<tempfile::TempDir>,
    /// The folder on Beta the files are dropped in.
    drop: Option<tempfile::TempDir>,
}

impl fmt::Debug for DragWorld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DragWorld({:?})", self.lab.names())
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

impl DragWorld {
    fn new() -> Self {
        Self {
            lab: Lab::new(),
            source: None,
            drop: None,
        }
    }

    /// Alpha and Beta on one desk, with files to drag in `source` and a
    /// folder on Beta to drop them in.
    async fn two(&mut self) {
        self.lab
            .form_desk(
                "Home office",
                "Alpha",
                &["Beta"],
                vec![("Alpha", Setup::default()), ("Beta", Setup::default())],
            )
            .await;
        self.source = Some(tempfile::tempdir().unwrap());
        self.drop = Some(tempfile::tempdir().unwrap());
    }

    fn source(&self) -> PathBuf {
        self.source.as_ref().unwrap().path().to_path_buf()
    }

    fn drop_dir(&self) -> PathBuf {
        self.drop.as_ref().unwrap().path().to_path_buf()
    }

    fn platform(&mut self, name: &str) -> VirtualPlatform {
        self.lab.node(name).platform.clone()
    }

    /// Alpha is dragging `names` out of its source folder.
    fn drag(&mut self, names: &[&str]) {
        let paths = names.iter().map(|n| self.source().join(n)).collect();
        self.platform("Alpha").script_outgoing_drag(Some(paths));
    }

    /// The pointer crosses to Beta, where the drag is dropped in the drop folder.
    async fn cross(&mut self) {
        let (alpha, beta) = (self.platform("Alpha"), self.platform("Beta"));
        beta.script_drop_target(self.drop_dir());
        alpha.send_input(LocalInput::Motion {
            dx: 1.0,
            dy: 0.0,
            abs: (1919.0, 540.0),
        });
        until("the pointer on Beta", || {
            alpha.is_grabbed()
                && beta
                    .injected()
                    .iter()
                    .any(|i| matches!(i, Injected::MotionAbs { .. }))
        })
        .await;
    }

    /// The `transfer` notifications Beta has shown so far.
    fn shown(&mut self) -> Vec<Value> {
        self.lab.node("Beta").events.all(Topic::Transfer)
    }

    async fn transfer_state(&mut self, state: &str) -> Value {
        self.lab
            .node("Beta")
            .notification(&format!("a transfer {state}"), Topic::Transfer, |p| {
                p["state"] == state
            })
            .await
    }
}

fn write(path: PathBuf, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

// ---- scenario 1: drag and drop ----------------------------------------------------------

#[given("the desk user is dragging files or folders")]
async fn dragging(w: &mut DragWorld) {
    w.two().await;
    write(w.source().join("project/readme.txt"), b"read me");
    write(w.source().join("project/src/main.rs"), b"fn main() {}");
    write(w.source().join("photo.png"), &[1, 2, 3, 4]);
    w.drag(&["project", "photo.png"]);
}

#[when("they cross to another computer and drop them on a folder or the desktop")]
async fn cross_and_drop(w: &mut DragWorld) {
    w.cross().await;
}

#[then("copies appear at the drop location")]
async fn copies_appear(w: &mut DragWorld) {
    w.transfer_state("completed").await;
    let drop = w.drop_dir();
    assert_eq!(
        std::fs::read(drop.join("project/readme.txt")).unwrap(),
        b"read me"
    );
    assert_eq!(
        std::fs::read(drop.join("project/src/main.rs")).unwrap(),
        b"fn main() {}"
    );
    assert_eq!(std::fs::read(drop.join("photo.png")).unwrap(), [1, 2, 3, 4]);
    // The platform was asked to drag what the user dragged, by name.
    assert_eq!(
        w.platform("Beta").incoming_drags(),
        [vec!["project".to_string(), "photo.png".to_string()]]
    );
    // Copies: the originals stay where they were.
    assert!(w.source().join("photo.png").exists());
}

// ---- scenario 2: transfer progress ------------------------------------------------------

const BIG: usize = 4 << 20;

#[given("a transfer is under way")]
async fn under_way(w: &mut DragWorld) {
    w.two().await;
    write(w.source().join("big.bin"), &vec![7u8; BIG]);
    w.drag(&["big.bin"]);
    w.cross().await;
}

#[when("the desk user looks at the screen")]
async fn looks_at_the_screen(w: &mut DragWorld) {
    // The app renders the `transfer` notifications; the next step reads them.
    w.transfer_state("completed").await;
}

#[then("it shows the file being copied, how much is done, the speed, and the time remaining")]
async fn shows_progress(w: &mut DragWorld) {
    let shown = w.shown();
    // The accept announces the transfer before any file has begun.
    let copying: Vec<_> = shown
        .iter()
        .filter(|n| n["state"] == "copying" && n["bytes_done"] != 0)
        .collect();
    assert!(!copying.is_empty(), "progress was shown: {shown:?}");
    assert!(copying.iter().all(|n| n["current_item"] == "big.bin"));
    assert!(copying.iter().all(|n| n["bytes_total"] == BIG));
    let last = shown.last().unwrap();
    assert_eq!(last["state"], "completed");
    assert_eq!(last["current_item"], "big.bin");
    assert_eq!(last["bytes_done"], BIG);
    assert!(last["rate"].as_u64().unwrap() > 0, "{last}");
    assert_eq!(last["eta_seconds"], 0);
}

// ---- scenario 3: name collision ---------------------------------------------------------

#[given("a dropped file has the same name as one already at the destination")]
async fn same_name(w: &mut DragWorld) {
    w.two().await;
    write(w.source().join("a.txt"), b"new a");
    write(w.source().join("b.txt"), b"new b");
    write(w.drop_dir().join("a.txt"), b"old a");
    w.drag(&["a.txt", "b.txt"]);
}

#[when("the transfer reaches it")]
async fn transfer_reaches_it(w: &mut DragWorld) {
    w.cross().await;
}

#[then("the desk user is asked whether to overwrite it")]
async fn asked(w: &mut DragWorld) {
    let question = w.transfer_state("awaiting_overwrite_decision").await;
    assert_eq!(question["item"], "a.txt");
    assert!(question["message"].as_str().unwrap().contains("a.txt"));
    // Nothing is replaced while the question stands.
    assert_eq!(std::fs::read(w.drop_dir().join("a.txt")).unwrap(), b"old a");
    // The user says no: that file is left alone and the rest is copied.
    let id = question["transfer_id"].as_str().unwrap();
    w.lab.node("Beta").control().resolve_transfer(id, false);
    w.transfer_state("completed").await;
    assert_eq!(std::fs::read(w.drop_dir().join("a.txt")).unwrap(), b"old a");
    assert_eq!(std::fs::read(w.drop_dir().join("b.txt")).unwrap(), b"new b");
}

// ---- scenario 4: not enough space -------------------------------------------------------

/// Unix file systems make the hole in `set_len` free by themselves; NTFS
/// allocates every byte unless the file is first marked sparse.
#[cfg(unix)]
fn make_sparse(_file: &std::fs::File) {}

#[cfg(windows)]
fn make_sparse(file: &std::fs::File) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// `FSCTL_SET_SPARSE`.
    const SET_SPARSE: u32 = 0x0009_00C4;
    let mut returned = 0u32;
    // SAFETY: the handle is open for writing and lives through the call; the
    // control code takes no input or output buffer.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            SET_SPARSE,
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    assert_ne!(ok, 0, "the test volume supports sparse files");
}

#[given("the destination does not have room for the transfer")]
async fn no_room(w: &mut DragWorld) {
    w.two().await;
    // A hole in a file costs no space here but counts in full for the check.
    let room = mouser_core::transfer::space::free_space(&w.drop_dir());
    let big = std::fs::File::create(w.source().join("huge.bin")).unwrap();
    make_sparse(&big);
    big.set_len(room + (1 << 20)).unwrap();
    w.drag(&["huge.bin"]);
}

#[when("the drop happens")]
async fn drop_happens(w: &mut DragWorld) {
    w.cross().await;
}

#[then("the desk user is told there is not enough space and nothing is copied")]
async fn told_no_space(w: &mut DragWorld) {
    let told = w.transfer_state("rejected").await;
    assert!(told["message"].as_str().unwrap().contains("not enough"));
    // The computer the files came from is told as well.
    let alpha = w
        .lab
        .node("Alpha")
        .notification("Alpha told", Topic::Transfer, |p| p["state"] == "rejected")
        .await;
    assert_eq!(alpha["reason"], "no_space");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(std::fs::read_dir(w.drop_dir()).unwrap().count(), 0);
}

// ---- scenario 5: cancelled drag ---------------------------------------------------------

#[given("the desk user has dragged files to another computer")]
async fn dragged_away(w: &mut DragWorld) {
    w.two().await;
    write(w.source().join("a.txt"), b"a");
    w.drag(&["a.txt"]);
    // Beta's user will cancel the drag there.
    w.platform("Beta").script_drag_cancelled(true);
}

#[when("they cancel the drag there or drop it on something that is not a folder")]
async fn cancel_the_drag(w: &mut DragWorld) {
    w.cross().await;
    let beta = w.platform("Beta");
    until("Beta's drag", || !beta.incoming_drags().is_empty()).await;
}

#[then(
    "nothing is copied, the progress display closes without an error, the computer the files came from is told the drag ended, and no problem is raised"
)]
async fn ended_quietly(w: &mut DragWorld) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(std::fs::read_dir(w.drop_dir()).unwrap().count(), 0);
    // No progress display, no failure on either computer.
    assert!(w.shown().is_empty(), "{:?}", w.shown());
    assert!(w.lab.node("Alpha").events.all(Topic::Transfer).is_empty());
    for name in ["Alpha", "Beta"] {
        // (Every virtual computer lists its standing platform limitations.)
        let problems = w.lab.node(name).status().await.problems;
        assert!(
            problems
                .iter()
                .all(|p| p.kind == ProblemKind::PlatformLimitation),
            "{name}: {problems:?}"
        );
    }
}

// ---- accepted limitation: Linux ---------------------------------------------------------

const LINUX: &str = "File drag between computers is unavailable on Linux. Copy and paste (Story 6) is the alternative.";

#[given("file drag between computers is unavailable on Linux")]
async fn unavailable_on_linux(w: &mut DragWorld) {
    let linux = VirtualPlatform::limited();
    linux.set_info(PlatformInfo {
        os: Os::Linux,
        version: "test".into(),
        session: SessionType::Native,
        limitations: vec![Limitation {
            capability: Capability::FileDrag,
            text: LINUX.into(),
        }],
    });
    let setup = Setup {
        platform: linux,
        ..Setup::default()
    };
    w.lab.add_with("Tux", setup).await;
}

#[when("the desk user looks at the app on a Linux computer")]
async fn looks_at_the_linux_app(_w: &mut DragWorld) {
    // The app renders `status`; the next step reads it.
}

#[then(
    "the app states that file drag between computers is unavailable and that copy and paste (Story 6) is the alternative"
)]
async fn app_states_it(w: &mut DragWorld) {
    let status = w.lab.node("Tux").status().await;
    assert!(
        status.computers[0].limitations.iter().any(|l| l == LINUX),
        "{:?}",
        status.computers[0].limitations
    );
    assert!(
        status
            .problems
            .iter()
            .any(|p| p.kind == ProblemKind::PlatformLimitation && p.message == LINUX)
    );
}
