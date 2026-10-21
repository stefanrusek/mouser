use crate::drag::{DragProbe, DragStarter, MacFileDrag, appkit_point, destination};
use mouser_platform::{DropTarget, Error, FileDrag};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

#[derive(Default)]
struct Probe {
    down: AtomicBool,
    files: Mutex<Vec<String>>,
    count: AtomicU64,
}

impl DragProbe for &Probe {
    fn button_down(&self) -> bool {
        self.down.load(Ordering::SeqCst)
    }
    fn files(&self) -> Vec<String> {
        self.files.lock().unwrap().clone()
    }
    fn change_count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }
}

type Started = Vec<(Vec<String>, (f64, f64))>;

#[derive(Default)]
struct Starter {
    started: Mutex<Started>,
    result: Mutex<Option<PathBuf>>,
}

impl DragStarter for &Starter {
    fn pointer(&self) -> ((f64, f64), f64) {
        ((100.0, 200.0), 1000.0)
    }
    fn run(&self, names: Vec<String>, at: (f64, f64)) -> mouser_platform::Result<PathBuf> {
        self.started.lock().unwrap().push((names, at));
        self.result
            .lock()
            .unwrap()
            .clone()
            .ok_or(Error::Os("drag cancelled".into()))
    }
}

fn drag<'a>(probe: &'a Probe, starter: &'a Starter) -> MacFileDrag<&'a Probe, &'a Starter> {
    MacFileDrag::new(probe, starter)
}

#[test]
fn outgoing_returns_the_file_urls_on_the_drag_pasteboard_while_a_drag_is_in_progress() {
    let (probe, starter) = (Probe::default(), Starter::default());
    let mut d = drag(&probe, &starter);
    assert_eq!(d.outgoing(), None);
    // A drag starts: button down and the drag pasteboard is rewritten.
    probe.down.store(true, Ordering::SeqCst);
    probe.count.store(1, Ordering::SeqCst);
    *probe.files.lock().unwrap() = vec![
        "file:///Users/a/My%20File.txt".into(),
        "/private/var/x.png".into(),
    ];
    assert_eq!(
        d.outgoing(),
        Some(vec![
            PathBuf::from("/Users/a/My File.txt"),
            PathBuf::from("/var/x.png"),
        ])
    );
}

#[test]
fn outgoing_ignores_a_stale_drag_pasteboard() {
    let (probe, starter) = (Probe::default(), Starter::default());
    let mut d = drag(&probe, &starter);
    probe.count.store(5, Ordering::SeqCst);
    *probe.files.lock().unwrap() = vec!["/old.txt".into()];
    assert_eq!(d.outgoing(), None);
    // Button goes down for something that is not a file drag: same counter.
    probe.down.store(true, Ordering::SeqCst);
    assert_eq!(d.outgoing(), None);
}

#[test]
fn outgoing_is_none_when_a_drag_carries_no_files() {
    let (probe, starter) = (Probe::default(), Starter::default());
    let mut d = drag(&probe, &starter);
    assert_eq!(d.outgoing(), None);
    probe.down.store(true, Ordering::SeqCst);
    probe.count.store(1, Ordering::SeqCst);
    assert_eq!(d.outgoing(), None);
}

#[test]
fn outgoing_reports_a_drag_seen_before_any_idle_poll() {
    let (probe, starter) = (Probe::default(), Starter::default());
    let mut d = drag(&probe, &starter);
    probe.down.store(true, Ordering::SeqCst);
    *probe.files.lock().unwrap() = vec!["/a.txt".into()];
    assert_eq!(d.outgoing(), Some(vec![PathBuf::from("/a.txt")]));
}

#[test]
fn begin_incoming_starts_a_drag_at_the_cursor_and_reports_the_destination_folder() {
    let (probe, starter) = (Probe::default(), Starter::default());
    *starter.result.lock().unwrap() = Some(PathBuf::from("/Users/a/Desktop/one.txt"));
    let mut d = drag(&probe, &starter);
    let target = d
        .begin_incoming(vec!["one.txt".into(), "two.txt".into()])
        .unwrap();
    assert_eq!(target, DropTarget(PathBuf::from("/Users/a/Desktop")));
    // Cursor (100, 200) from the top of a 1000 pt display is y = 800 from the bottom.
    assert_eq!(
        *starter.started.lock().unwrap(),
        [(
            vec!["one.txt".to_string(), "two.txt".to_string()],
            (100.0, 800.0)
        )]
    );
}

#[test]
fn begin_incoming_without_names_or_without_a_drop_is_an_error() {
    let (probe, starter) = (Probe::default(), Starter::default());
    let mut d = drag(&probe, &starter);
    assert_eq!(
        d.begin_incoming(vec![]),
        Err(Error::Os("no files to drag".into()))
    );
    assert!(starter.started.lock().unwrap().is_empty());
    assert_eq!(
        d.begin_incoming(vec!["a".into()]),
        Err(Error::Os("drag cancelled".into()))
    );
}

#[test]
fn coordinate_flip_and_destination() {
    assert_eq!(appkit_point((3.0, 0.0), 900.0), (3.0, 900.0));
    assert_eq!(appkit_point((3.0, 900.0), 900.0), (3.0, 0.0));
    assert_eq!(destination(Path::new("/a/b/c.txt")), PathBuf::from("/a/b"));
}
