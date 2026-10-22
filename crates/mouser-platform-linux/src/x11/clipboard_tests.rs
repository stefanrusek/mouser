use super::clipboard::X11Clipboard;
use super::testserver::Xvfb;
use super::{connect, create_stop_window, wake};
use mouser_platform::{ClipRep, Clipboard};
use std::collections::HashMap;
use std::time::Duration;
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, EventMask, PropMode, SELECTION_NOTIFY_EVENT,
    SelectionNotifyEvent, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

fn atom(conn: &RustConnection, name: &str) -> Atom {
    conn.intern_atom(false, name.as_bytes())
        .unwrap()
        .reply()
        .unwrap()
        .atom
}

/// A second X client, as another application would be.
struct Client {
    env: HashMap<String, String>,
    conn: RustConnection,
    window: Window,
}

impl Client {
    fn new(server: &Xvfb) -> Client {
        let env = server.env();
        let (conn, root) = connect(&env);
        let window = create_stop_window(&conn, root);
        Client { env, conn, window }
    }

    /// Takes the clipboard (`own`) or gives it up, as another app would.
    fn set_owner(&self, own: bool) {
        let owner = if own { self.window } else { 0 };
        self.conn
            .set_selection_owner(owner, atom(&self.conn, "CLIPBOARD"), CURRENT_TIME)
            .unwrap();
        self.conn.get_input_focus().unwrap().reply().unwrap();
    }

    /// Asks the owner for one target the way a paste would; `None` when refused.
    fn request(&self, target: &str) -> Option<Vec<u8>> {
        self.convert(target, atom(&self.conn, "PASTE"))
    }

    /// Like [`Client::request`] from an old client that names no property
    /// (`NONE`): the owner answers in a property named after the target.
    fn request_without_a_property(&self, target: &str) -> Option<Vec<u8>> {
        self.convert(target, 0)
    }

    fn convert(&self, target: &str, requested: Atom) -> Option<Vec<u8>> {
        self.conn
            .convert_selection(
                self.window,
                atom(&self.conn, "CLIPBOARD"),
                atom(&self.conn, target),
                requested,
                CURRENT_TIME,
            )
            .unwrap();
        self.conn.flush().unwrap();
        loop {
            if let Event::SelectionNotify(n) = self.conn.wait_for_event().unwrap() {
                if n.property == 0 {
                    return None;
                }
                let reply = self
                    .conn
                    .get_property(
                        true,
                        self.window,
                        n.property,
                        AtomEnum::ANY,
                        0,
                        u32::MAX / 4,
                    )
                    .unwrap()
                    .reply()
                    .unwrap();
                return Some(reply.value);
            }
        }
    }

    /// Serves `text` for `UTF8_STRING` on a thread, listing it in `TARGETS`;
    /// refuses the request itself when `refuse`. Dropping the result ends it.
    fn own_with_text(self, text: &'static str, mode: Mode) -> Owner {
        self.set_owner(true);
        let root = self.conn.setup().roots[0].root;
        let (stop, env) = (create_stop_window(&self.conn, root), self.env.clone());
        let thread = std::thread::spawn(move || {
            let (utf8, targets) = (atom(&self.conn, "UTF8_STRING"), atom(&self.conn, "TARGETS"));
            loop {
                let Event::SelectionRequest(r) = self.conn.wait_for_event().unwrap() else {
                    return;
                };
                let property = if r.target == targets {
                    self.conn
                        .change_property32(
                            PropMode::REPLACE,
                            r.requestor,
                            r.property,
                            AtomEnum::ATOM,
                            &[targets, utf8],
                        )
                        .unwrap();
                    r.property
                } else if r.target == utf8 && mode == Mode::StartIncr {
                    self.conn
                        .change_property32(
                            PropMode::REPLACE,
                            r.requestor,
                            r.property,
                            atom(&self.conn, "INCR"),
                            &[text.len() as u32],
                        )
                        .unwrap();
                    r.property
                } else if r.target == utf8 && mode == Mode::Serve {
                    self.conn
                        .change_property8(
                            PropMode::REPLACE,
                            r.requestor,
                            r.property,
                            utf8,
                            text.as_bytes(),
                        )
                        .unwrap();
                    r.property
                } else {
                    0
                };
                let event = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: r.time,
                    requestor: r.requestor,
                    selection: r.selection,
                    target: r.target,
                    property,
                };
                self.conn
                    .send_event(false, r.requestor, EventMask::NO_EVENT, event)
                    .unwrap();
                self.conn.flush().unwrap();
            }
        });
        Owner {
            thread: Some(thread),
            env,
            stop,
        }
    }
}

/// How a test owner answers a request for its text.
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Serve,
    Refuse,
    /// Announces an `INCR` transfer and never sends a chunk.
    StartIncr,
}

/// The thread serving another client's clipboard; dropping it ends the thread.
struct Owner {
    thread: Option<std::thread::JoinHandle<()>>,
    env: HashMap<String, String>,
    stop: Window,
}

impl Drop for Owner {
    fn drop(&mut self) {
        wake(&self.env, self.stop);
        self.thread.take().unwrap().join().unwrap();
    }
}

fn clipboard(server: &Xvfb) -> X11Clipboard {
    X11Clipboard::new(&server.env())
}

#[test]
fn everything_written_is_read_back() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    let reps = vec![
        ClipRep::Text("mouser ünïcode".into()),
        ClipRep::Html("<i>hi</i>".into()),
        ClipRep::Rtf(b"{\\rtf1 hi}".to_vec()),
        ClipRep::Png(vec![0x89, b'P', b'N', b'G', 0, 1, 2]),
        ClipRep::Files(vec!["/tmp/a b.txt".into(), "/tmp/c.png".into()]),
    ];
    clip.write(reps.clone()).unwrap();
    assert_eq!(clip.read().unwrap(), reps);
}

#[test]
fn a_second_client_sees_the_targets_and_the_data() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    clip.write(vec![
        ClipRep::Text("plain".into()),
        ClipRep::Files(vec!["/x/y".into()]),
    ])
    .unwrap();
    let other = Client::new(&server);
    let targets = other.request("TARGETS").unwrap();
    let names: Vec<Atom> = targets
        .as_chunks::<4>()
        .0
        .iter()
        .map(|a| u32::from_ne_bytes(*a))
        .collect();
    for name in [
        "UTF8_STRING",
        "text/uri-list",
        "x-special/gnome-copied-files",
    ] {
        assert!(names.contains(&atom(&other.conn, name)), "{name}");
    }
    assert_eq!(other.request("UTF8_STRING").unwrap(), b"plain");
    assert_eq!(other.request("text/uri-list").unwrap(), b"file:///x/y\r\n");
    assert_eq!(other.request("text/html"), None);
}

#[test]
fn a_request_naming_no_property_is_answered_in_the_target() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    clip.write(vec![ClipRep::Text("plain".into())]).unwrap();
    let other = Client::new(&server);
    assert_eq!(
        other.request_without_a_property("UTF8_STRING").unwrap(),
        b"plain"
    );
}

#[test]
fn large_data_goes_in_chunks_and_arrives_whole() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    let png: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    clip.write(vec![ClipRep::Png(png.clone())]).unwrap();
    assert_eq!(clip.read().unwrap(), [ClipRep::Png(png)]);
    // Data of exactly one chunk still ends cleanly.
    let exact = vec![7u8; 64 * 1024 * 2];
    clip.write(vec![ClipRep::Png(exact.clone())]).unwrap();
    assert_eq!(clip.read().unwrap(), [ClipRep::Png(exact)]);
}

#[test]
fn the_change_count_follows_our_writes_and_other_clients() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    let start = clip.change_count();
    clip.write(vec![ClipRep::Text("one".into())]).unwrap();
    assert_eq!(clip.change_count(), start + 1);
    clip.write(vec![ClipRep::Text("two".into())]).unwrap();
    assert_eq!(clip.change_count(), start + 2);
    let other = Client::new(&server);
    other.set_owner(true);
    mouser_wait(|| clip.change_count() == start + 3);
    other.set_owner(false);
    mouser_wait(|| clip.change_count() == start + 4);
    // The old offer is gone once another client owned the clipboard.
    assert_eq!(clip.read().unwrap(), []);
}

fn mouser_wait(mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(std::time::Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn an_empty_clipboard_reads_as_nothing() {
    let server = Xvfb::start();
    assert_eq!(clipboard(&server).read().unwrap(), []);
}

#[test]
fn another_clients_text_is_read() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    let owner = Client::new(&server).own_with_text("from elsewhere", Mode::Serve);
    assert_eq!(
        clip.read().unwrap(),
        [ClipRep::Text("from elsewhere".into())]
    );
    drop(owner);
}

#[test]
fn a_target_the_owner_lists_but_refuses_reads_as_absent() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    let owner = Client::new(&server).own_with_text("never given", Mode::Refuse);
    assert_eq!(clip.read().unwrap(), []);
    drop(owner);
}

#[test]
fn an_owner_that_starts_an_incr_transfer_and_stops_fails_the_read() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    clip.answer_within = Duration::from_millis(200);
    let owner = Client::new(&server).own_with_text("long", Mode::StartIncr);
    let error = clip.read().unwrap_err();
    assert_eq!(error.to_string(), "the clipboard owner did not answer");
    drop(owner);
}

#[test]
fn an_owner_that_never_answers_fails_the_read() {
    let server = Xvfb::start();
    let mut clip = clipboard(&server);
    clip.answer_within = Duration::from_millis(100);
    let other = Client::new(&server);
    other.set_owner(true);
    let error = clip.read().unwrap_err();
    assert_eq!(error.to_string(), "the clipboard owner did not answer");
}
