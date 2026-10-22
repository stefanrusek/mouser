//! The X11 clipboard: ownership of the `CLIPBOARD` selection with `TARGETS`
//! and `INCR` for large data, the XFixes owner-change counter, and reads
//! from whichever client owns the selection.
//!
//! Writes serve requests on a thread with its own connection; every read
//! uses a short-lived connection so it never competes with that thread.
//! Remote files reach this backend already staged on the local disk (core
//! fetches them into its cache directory), so only paths are offered.

use super::{connect, create_stop_window, wake};
use crate::clipboard::{X11_TEXT, decode, encode, mime_types};
use mouser_platform::{ClipRep, Clipboard, Error, Result};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use x11rb::CURRENT_TIME;
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEventMask};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, EventMask, PropMode, Property,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, Window,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

/// The largest value sent in one property; bigger data goes by `INCR`.
const CHUNK: usize = 64 * 1024;

/// How long a read waits for the owner to answer.
const ANSWER: Duration = Duration::from_secs(5);

/// Atoms by name, interned once.
#[derive(Clone)]
struct Atoms(Arc<HashMap<String, Atom>>);

impl Atoms {
    fn intern(conn: &RustConnection) -> Atoms {
        let mut names: Vec<&str> = mime_types(X11_TEXT).to_vec();
        names.extend(["CLIPBOARD", "TARGETS", "INCR", "MOUSER_CLIPBOARD"]);
        let cookies: Vec<_> = names
            .iter()
            .map(|n| conn.intern_atom(false, n.as_bytes()).unwrap())
            .collect();
        Atoms(Arc::new(
            names
                .into_iter()
                .zip(cookies)
                .map(|(n, c)| (n.to_string(), c.reply().unwrap().atom))
                .collect(),
        ))
    }

    fn get(&self, name: &str) -> Atom {
        self.0[name]
    }
}

/// One `INCR` transfer in progress to a requestor.
struct Incr {
    requestor: Window,
    property: Atom,
    target: Atom,
    data: Vec<u8>,
    sent: usize,
}

/// What this clipboard offers while it owns the selection.
type Offer = Arc<Mutex<Vec<(Atom, Vec<u8>)>>>;

struct Server {
    conn: Arc<RustConnection>,
    window: Window,
    stop: Window,
    atoms: Atoms,
    offer: Offer,
    changes: Arc<AtomicU64>,
    transfers: Vec<Incr>,
}

impl Server {
    fn run(mut self) {
        loop {
            match self.conn.wait_for_event().unwrap() {
                Event::SelectionRequest(request) => self.answer(request),
                Event::SelectionClear(_) => self.offer.lock().unwrap().clear(),
                Event::XfixesSelectionNotify(change) if change.owner != self.window => {
                    self.changes.fetch_add(1, Ordering::SeqCst);
                }
                Event::PropertyNotify(change) if change.state == Property::DELETE => {
                    self.next_chunk(change.window, change.atom)
                }
                Event::ClientMessage(message) if message.window == self.stop => return,
                _ => {}
            }
        }
    }

    fn notify(&self, request: &SelectionRequestEvent, property: Atom) {
        let event = SelectionNotifyEvent {
            response_type: SELECTION_NOTIFY_EVENT,
            sequence: 0,
            time: request.time,
            requestor: request.requestor,
            selection: request.selection,
            target: request.target,
            property,
        };
        self.conn
            .send_event(false, request.requestor, EventMask::NO_EVENT, event)
            .unwrap();
        self.conn.flush().unwrap();
    }

    /// Answers one `SelectionRequest`: the target list, the data, or a refusal.
    fn answer(&mut self, request: SelectionRequestEvent) {
        let property = if request.property == 0 {
            request.target
        } else {
            request.property
        };
        let offer = self.offer.lock().unwrap().clone();
        let data = offer.iter().find(|(t, _)| *t == request.target);
        let mut sent = property;
        if request.target == self.atoms.get("TARGETS") {
            let mut targets = vec![request.target];
            targets.extend(offer.iter().map(|(t, _)| *t));
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    AtomEnum::ATOM,
                    &targets,
                )
                .unwrap();
        } else if let Some((target, data)) = data.filter(|(_, d)| d.len() <= CHUNK) {
            self.conn
                .change_property8(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    *target,
                    data,
                )
                .unwrap();
        } else if let Some((target, data)) = data {
            self.conn
                .change_window_attributes(
                    request.requestor,
                    &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
                )
                .unwrap();
            self.conn
                .change_property32(
                    PropMode::REPLACE,
                    request.requestor,
                    property,
                    self.atoms.get("INCR"),
                    &[data.len() as u32],
                )
                .unwrap();
            self.transfers.push(Incr {
                requestor: request.requestor,
                property,
                target: *target,
                data: data.clone(),
                sent: 0,
            });
        } else {
            sent = 0;
        }
        self.notify(&request, sent);
    }

    /// The requestor took the last chunk: send the next, or the empty one
    /// that ends the transfer.
    fn next_chunk(&mut self, window: Window, property: Atom) {
        let Some(at) = self
            .transfers
            .iter()
            .position(|t| t.requestor == window && t.property == property)
        else {
            return;
        };
        let transfer = &mut self.transfers[at];
        let end = (transfer.sent + CHUNK).min(transfer.data.len());
        let chunk = &transfer.data[transfer.sent..end];
        self.conn
            .change_property8(PropMode::REPLACE, window, property, transfer.target, chunk)
            .unwrap();
        self.conn.flush().unwrap();
        transfer.sent = end;
        if chunk.is_empty() {
            self.transfers.remove(at);
        }
    }
}

pub(crate) struct X11Clipboard {
    env: HashMap<String, String>,
    conn: Arc<RustConnection>,
    window: Window,
    stop: Window,
    atoms: Atoms,
    offer: Offer,
    changes: Arc<AtomicU64>,
    server: Option<JoinHandle<()>>,
    /// How long a read waits for the owner; tests shorten it.
    pub(crate) answer_within: Duration,
}

impl X11Clipboard {
    pub(crate) fn new(env: &HashMap<String, String>) -> Self {
        let (conn, root) = connect(env);
        let conn = Arc::new(conn);
        conn.xfixes_query_version(5, 0).unwrap().reply().unwrap();
        let atoms = Atoms::intern(&conn);
        let window = create_stop_window(&conn, root);
        let stop = create_stop_window(&conn, root);
        conn.xfixes_select_selection_input(
            window,
            atoms.get("CLIPBOARD"),
            SelectionEventMask::SET_SELECTION_OWNER
                | SelectionEventMask::SELECTION_WINDOW_DESTROY
                | SelectionEventMask::SELECTION_CLIENT_CLOSE,
        )
        .unwrap();
        conn.get_input_focus().unwrap().reply().unwrap();
        let offer = Offer::default();
        let changes = Arc::new(AtomicU64::new(0));
        let server = Server {
            conn: conn.clone(),
            window,
            stop,
            atoms: atoms.clone(),
            offer: offer.clone(),
            changes: changes.clone(),
            transfers: Vec::new(),
        };
        X11Clipboard {
            env: env.clone(),
            conn,
            window,
            stop,
            atoms,
            offer,
            changes,
            server: Some(std::thread::spawn(move || server.run())),
            answer_within: ANSWER,
        }
    }
}

/// What `accept` makes of the next event it takes, or an error when none
/// comes in time.
fn wait_for<T>(
    conn: &RustConnection,
    within: Duration,
    accept: impl Fn(Event) -> Option<T>,
) -> Result<T> {
    let deadline = Instant::now() + within;
    loop {
        if let Some(event) = conn.poll_for_event().unwrap() {
            if let Some(taken) = accept(event) {
                return Ok(taken);
            }
        } else if Instant::now() >= deadline {
            return Err(Error::Os("the clipboard owner did not answer".into()));
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

/// One read connection: asks the owner of `CLIPBOARD` for targets.
struct Reader {
    conn: RustConnection,
    window: Window,
    atoms: Atoms,
    within: Duration,
}

impl Reader {
    /// The bytes of `target`, or `None` when the owner refuses it.
    fn fetch(&self, target: Atom) -> Result<Option<Vec<u8>>> {
        let property = self.atoms.get("MOUSER_CLIPBOARD");
        self.conn
            .convert_selection(
                self.window,
                self.atoms.get("CLIPBOARD"),
                target,
                property,
                CURRENT_TIME,
            )
            .unwrap();
        self.conn.flush().unwrap();
        let answer = wait_for(&self.conn, self.within, |e| match e {
            Event::SelectionNotify(n) if n.requestor == self.window => Some(n),
            _ => None,
        })?;
        if answer.property == 0 {
            return Ok(None);
        }
        let first = self.take(property);
        if first.type_ != self.atoms.get("INCR") {
            return Ok(Some(first.value));
        }
        let mut data = Vec::new();
        loop {
            wait_for(&self.conn, self.within, |e| match e {
                Event::PropertyNotify(p)
                    if p.window == self.window
                        && p.atom == property
                        && p.state == Property::NEW_VALUE =>
                {
                    Some(())
                }
                _ => None,
            })?;
            let chunk = self.take(property).value;
            if chunk.is_empty() {
                return Ok(Some(data));
            }
            data.extend(chunk);
        }
    }

    /// Reads and deletes the property.
    fn take(&self, property: Atom) -> x11rb::protocol::xproto::GetPropertyReply {
        self.conn
            .get_property(true, self.window, property, AtomEnum::ANY, 0, u32::MAX / 4)
            .unwrap()
            .reply()
            .unwrap()
    }
}

impl Clipboard for X11Clipboard {
    fn read(&mut self) -> Result<Vec<ClipRep>> {
        let (conn, root) = connect(&self.env);
        let window = create_stop_window(&conn, root);
        conn.change_window_attributes(
            window,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
        )
        .unwrap();
        let reader = Reader {
            conn,
            window,
            atoms: self.atoms.clone(),
            within: self.answer_within,
        };
        let Some(listed) = reader.fetch(self.atoms.get("TARGETS"))? else {
            return Ok(Vec::new());
        };
        let offered: Vec<Atom> = listed
            .as_chunks::<4>()
            .0
            .iter()
            .map(|a| u32::from_ne_bytes(*a))
            .collect();
        decode(X11_TEXT, |mime| {
            let target = self.atoms.get(mime);
            if offered.contains(&target) {
                reader.fetch(target)
            } else {
                Ok(None)
            }
        })
    }

    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()> {
        *self.offer.lock().unwrap() = encode(X11_TEXT, &reps)
            .into_iter()
            .map(|(mime, data)| (self.atoms.get(&mime), data))
            .collect();
        self.conn
            .set_selection_owner(self.window, self.atoms.get("CLIPBOARD"), CURRENT_TIME)
            .unwrap();
        let owner = self
            .conn
            .get_selection_owner(self.atoms.get("CLIPBOARD"))
            .unwrap()
            .reply()
            .unwrap()
            .owner;
        assert_eq!(owner, self.window, "the X server refused the selection");
        self.changes.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn change_count(&mut self) -> u64 {
        self.changes.load(Ordering::SeqCst)
    }
}

impl Drop for X11Clipboard {
    fn drop(&mut self) {
        wake(&self.env, self.stop);
        self.server.take().unwrap().join().unwrap();
    }
}
