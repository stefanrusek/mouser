//! A fake `org.freedesktop.portal.Desktop` speaking the Request/Response
//! protocol the real portals use.

use crate::daemon::FAKES;
use crate::eis::FakeEis;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use zbus::ObjectServer;
use zbus::message::Header;
use zbus::zvariant::{ObjectPath, OwnedFd, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, interface};

#[derive(Clone)]
pub struct InputCaptureConfig {
    pub version: u32,
    pub capabilities: u32,
    /// Restore token returned by `Start` (version 2 only).
    pub token: Option<&'static str>,
    /// The zones `GetZones` reports: `(width, height, x, y)`.
    pub zones: Vec<(u32, u32, i32, i32)>,
    /// Barrier ids `SetPointerBarriers` refuses.
    pub failed_barriers: Vec<u32>,
}

impl InputCaptureConfig {
    /// One 1920x1080 display at the origin, no refused barriers.
    pub fn new(version: u32, capabilities: u32, token: Option<&'static str>) -> Self {
        InputCaptureConfig {
            version,
            capabilities,
            token,
            zones: vec![(1920, 1080, 0, 0)],
            failed_barriers: vec![],
        }
    }
}

#[derive(Clone, Default)]
pub struct PortalConfig {
    pub remote_desktop: bool,
    pub clipboard: bool,
    pub registry: bool,
    pub input_capture: Option<InputCaptureConfig>,
    /// How long a user takes to answer a dialog.
    pub dialog_ms: u64,
    /// The RemoteDesktop portal returns no restore token.
    pub no_rd_token: bool,
    /// The desktop's libei endpoint offers no keyboard device.
    pub no_keyboard: bool,
    /// The RemoteDesktop grant includes the clipboard.
    pub clipboard_enabled: bool,
}

#[derive(Clone, Default)]
pub struct Calls(Arc<Mutex<Vec<String>>>);

impl Calls {
    fn push(&self, call: impl Into<String>) {
        self.0.lock().unwrap().push(call.into());
    }

    pub fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    pub fn contains(&self, call: &str) -> bool {
        self.all().iter().any(|c| c == call)
    }
}

fn ov<'a>(value: impl Into<Value<'a>>) -> OwnedValue {
    OwnedValue::try_from(value.into()).unwrap()
}

fn text(options: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    options
        .get(key)
        .map(|v| String::try_from(v.clone()).unwrap())
}

fn escape(sender: &str) -> String {
    sender.trim_start_matches(':').replace('.', "_")
}

#[derive(Clone)]
struct Shared {
    calls: Calls,
    dialog_ms: u64,
    no_rd_token: bool,
    no_keyboard: bool,
    clipboard_enabled: bool,
    /// What other applications have on the clipboard, by MIME type.
    contents: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    /// The read ends of the pipes handed out by SelectionWrite, by serial.
    writes: Arc<Mutex<HashMap<u32, std::io::PipeReader>>>,
    /// RemoteDesktop restore tokens handed out so far.
    issued: Arc<Mutex<u32>>,
    /// The libei servers handed out through ConnectToEIS, oldest first.
    eis: Arc<Mutex<Vec<Arc<FakeEis>>>>,
    /// The session paths created, oldest first.
    sessions: Arc<Mutex<Vec<String>>>,
}

impl Shared {
    /// Answers a request after the dialog delay, as a real portal does.
    fn respond(
        &self,
        conn: &Connection,
        header: &Header<'_>,
        options: &HashMap<String, OwnedValue>,
        results: HashMap<String, OwnedValue>,
        done: &'static str,
    ) -> OwnedObjectPath {
        let sender = escape(header.sender().unwrap().as_str());
        let path = format!(
            "/org/freedesktop/portal/desktop/request/{sender}/{}",
            text(options, "handle_token").unwrap()
        );
        let (conn, delay, calls, signal_path) = (
            conn.clone(),
            self.dialog_ms,
            self.calls.clone(),
            path.clone(),
        );
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(delay)).await;
            conn.emit_signal(
                None::<&str>,
                signal_path,
                "org.freedesktop.portal.Request",
                "Response",
                &(0u32, results),
            )
            .await
            .unwrap();
            calls.push(done);
        });
        OwnedObjectPath::try_from(path).unwrap()
    }

    /// A fresh libei connection: the client gets one end, a fake server the other.
    fn connect_to_eis(&self) -> OwnedFd {
        let (client, server) = std::os::unix::net::UnixStream::pair().unwrap();
        self.eis
            .lock()
            .unwrap()
            .push(Arc::new(FakeEis::serve(server, !self.no_keyboard)));
        OwnedFd::from(std::os::fd::OwnedFd::from(client))
    }

    async fn open_session(
        &self,
        server: &ObjectServer,
        header: &Header<'_>,
        options: &HashMap<String, OwnedValue>,
    ) -> String {
        let path = Shared::session_path(header, options);
        server
            .at(path.as_str(), Session(self.calls.clone()))
            .await
            .unwrap();
        self.sessions.lock().unwrap().push(path.clone());
        path
    }

    fn session_path(header: &Header<'_>, options: &HashMap<String, OwnedValue>) -> String {
        format!(
            "/org/freedesktop/portal/desktop/session/{}/{}",
            escape(header.sender().unwrap().as_str()),
            text(options, "session_handle_token").unwrap()
        )
    }
}

struct Session(Calls);

#[interface(name = "org.freedesktop.portal.Session")]
impl Session {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    fn close(&self) {
        self.0.push("session.close");
    }
}

struct RemoteDesktop(Shared);

#[interface(name = "org.freedesktop.portal.RemoteDesktop")]
impl RemoteDesktop {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        2
    }

    #[zbus(property, name = "AvailableDeviceTypes")]
    fn available_device_types(&self) -> u32 {
        3
    }

    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> OwnedObjectPath {
        self.0.calls.push("rd.create_session");
        let handle = self.0.open_session(server, &header, &options).await;
        let results = HashMap::from([("session_handle".to_string(), ov(handle))]);
        self.0
            .respond(conn, &header, &options, results, "rd.create_session.done")
    }

    async fn select_devices(
        &self,
        _session: ObjectPath<'_>,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> OwnedObjectPath {
        let persist = options
            .get("persist_mode")
            .map(|v| u32::try_from(v.clone()).unwrap());
        self.0.calls.push(format!(
            "rd.select_devices persist_mode={persist:?} token={:?}",
            text(&options, "restore_token")
        ));
        self.0.respond(
            conn,
            &header,
            &options,
            HashMap::new(),
            "rd.select_devices.done",
        )
    }

    async fn start(
        &self,
        _session: ObjectPath<'_>,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> OwnedObjectPath {
        self.0.calls.push("rd.start");
        let token = {
            let mut issued = self.0.issued.lock().unwrap();
            *issued += 1;
            format!("rd-token-{issued}")
        };
        let mut results = HashMap::from([
            ("devices".to_string(), ov(3u32)),
            (
                "clipboard_enabled".to_string(),
                ov(self.0.clipboard_enabled),
            ),
        ]);
        if !self.0.no_rd_token {
            results.insert("restore_token".to_string(), ov(token));
        }
        self.0
            .respond(conn, &header, &options, results, "rd.start.done")
    }

    #[zbus(name = "ConnectToEIS")]
    fn connect_to_eis(
        &self,
        _session: ObjectPath<'_>,
        _options: HashMap<String, OwnedValue>,
    ) -> OwnedFd {
        self.0.calls.push("rd.connect_to_eis");
        self.0.connect_to_eis()
    }
}

struct ClipboardPortal(Shared);

#[interface(name = "org.freedesktop.portal.Clipboard")]
impl ClipboardPortal {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        1
    }

    fn request_clipboard(&self, _session: ObjectPath<'_>, _options: HashMap<String, OwnedValue>) {
        self.0.calls.push("clipboard.request");
    }

    fn set_selection(&self, _session: ObjectPath<'_>, options: HashMap<String, OwnedValue>) {
        let mimes: Vec<String> = options
            .get("mime_types")
            .map(|v| Vec::<String>::try_from(v.try_clone().unwrap()).unwrap())
            .unwrap_or_default();
        self.0
            .calls
            .push(format!("clipboard.set_selection {}", mimes.join(",")));
    }

    fn selection_read(
        &self,
        _session: ObjectPath<'_>,
        mime_type: String,
    ) -> zbus::fdo::Result<OwnedFd> {
        let data = self
            .0
            .contents
            .lock()
            .unwrap()
            .get(&mime_type)
            .cloned()
            .ok_or_else(|| zbus::fdo::Error::Failed(format!("nothing offered as {mime_type}")))?;
        self.0.calls.push(format!("clipboard.read {mime_type}"));
        let (reader, mut writer) = std::io::pipe().unwrap();
        std::thread::spawn(move || {
            use std::io::Write;
            writer.write_all(&data).unwrap();
        });
        Ok(OwnedFd::from(std::os::fd::OwnedFd::from(reader)))
    }

    fn selection_write(&self, _session: ObjectPath<'_>, serial: u32) -> OwnedFd {
        let (reader, writer) = std::io::pipe().unwrap();
        self.0.writes.lock().unwrap().insert(serial, reader);
        OwnedFd::from(std::os::fd::OwnedFd::from(writer))
    }

    fn selection_write_done(&self, _session: ObjectPath<'_>, serial: u32, success: bool) {
        self.0
            .calls
            .push(format!("clipboard.write_done {serial} {success}"));
    }
}

struct Registry(Calls);

#[interface(name = "org.freedesktop.host.portal.Registry")]
impl Registry {
    fn register(&self, app_id: String, _options: HashMap<String, OwnedValue>) {
        self.0.push(format!("registry.register {app_id}"));
    }
}

struct InputCapture(Shared, InputCaptureConfig);

#[interface(name = "org.freedesktop.portal.InputCapture")]
impl InputCapture {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        self.1.version
    }

    #[zbus(property, name = "SupportedCapabilities")]
    fn supported_capabilities(&self) -> u32 {
        self.1.capabilities
    }

    async fn create_session(
        &self,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> OwnedObjectPath {
        self.0.calls.push("ic.create_session");
        let handle = self.0.open_session(server, &header, &options).await;
        let results = HashMap::from([
            (
                "session_handle".to_string(),
                ov(ObjectPath::try_from(handle).unwrap()),
            ),
            ("capabilities".to_string(), ov(3u32)),
        ]);
        self.0
            .respond(conn, &header, &options, results, "ic.create_session.done")
    }

    async fn create_session2(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> HashMap<String, OwnedValue> {
        self.0.calls.push("ic.create_session2");
        let handle = self.0.open_session(server, &header, &options).await;
        HashMap::from([(
            "session_handle".to_string(),
            ov(ObjectPath::try_from(handle).unwrap()),
        )])
    }

    async fn start(
        &self,
        _session: ObjectPath<'_>,
        _parent_window: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> OwnedObjectPath {
        let persist = options
            .get("persist_mode")
            .map(|v| u32::try_from(v.clone()).unwrap());
        self.0
            .calls
            .push(format!("ic.start persist_mode={persist:?}"));
        self.0.calls.push(format!(
            "ic.start.token {:?}",
            text(&options, "restore_token")
        ));
        let mut results = HashMap::from([("capabilities".to_string(), ov(3u32))]);
        if let Some(token) = self.1.token {
            results.insert("restore_token".to_string(), ov(token));
        }
        self.0
            .respond(conn, &header, &options, results, "ic.start.done")
    }

    async fn get_zones(
        &self,
        _session: ObjectPath<'_>,
        options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> OwnedObjectPath {
        self.0.calls.push("ic.get_zones");
        let zones: Vec<(u32, u32, i32, i32)> = self.1.zones.clone();
        let results = HashMap::from([
            ("zones".to_string(), ov(zones)),
            ("zone_set".to_string(), ov(7u32)),
        ]);
        self.0
            .respond(conn, &header, &options, results, "ic.get_zones.done")
    }

    async fn set_pointer_barriers(
        &self,
        _session: ObjectPath<'_>,
        options: HashMap<String, OwnedValue>,
        barriers: Vec<HashMap<String, OwnedValue>>,
        zone_set: u32,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] conn: &Connection,
    ) -> OwnedObjectPath {
        for barrier in barriers {
            let id = u32::try_from(barrier["barrier_id"].clone()).unwrap();
            let line = <(i32, i32, i32, i32)>::try_from(barrier["position"].clone()).unwrap();
            self.0.calls.push(format!(
                "ic.barrier zone_set={zone_set} id={id} line={line:?}"
            ));
        }
        let failed: Vec<u32> = self.1.failed_barriers.clone();
        let results = HashMap::from([("failed_barriers".to_string(), ov(failed))]);
        self.0.respond(
            conn,
            &header,
            &options,
            results,
            "ic.set_pointer_barriers.done",
        )
    }

    fn enable(&self, _session: ObjectPath<'_>, _options: HashMap<String, OwnedValue>) {
        self.0.calls.push("ic.enable");
    }

    fn release(&self, _session: ObjectPath<'_>, options: HashMap<String, OwnedValue>) {
        let id = options
            .get("activation_id")
            .map(|v| u32::try_from(v.clone()).unwrap());
        let at = options
            .get("cursor_position")
            .map(|v| <(f64, f64)>::try_from(v.clone()).unwrap());
        self.0.calls.push(format!("ic.release id={id:?} at={at:?}"));
    }

    #[zbus(name = "ConnectToEIS")]
    fn connect_to_eis(
        &self,
        _session: ObjectPath<'_>,
        _options: HashMap<String, OwnedValue>,
    ) -> OwnedFd {
        self.0.calls.push("ic.connect_to_eis");
        self.0.connect_to_eis()
    }
}

/// Keeps the fake portal on the bus while alive.
pub struct Portals {
    conn: Connection,
    pub calls: Calls,
    shared: Shared,
}

impl Portals {
    pub fn start(address: &str, config: PortalConfig) -> Portals {
        let calls = Calls::default();
        let shared = Shared {
            calls: calls.clone(),
            dialog_ms: config.dialog_ms,
            no_rd_token: config.no_rd_token,
            no_keyboard: config.no_keyboard,
            clipboard_enabled: config.clipboard_enabled,
            contents: Arc::default(),
            writes: Arc::default(),
            issued: Arc::default(),
            eis: Arc::default(),
            sessions: Arc::default(),
        };
        let shared_copy = shared.clone();
        let path = "/org/freedesktop/portal/desktop";
        let mut builder = zbus::connection::Builder::address(address)
            .unwrap()
            .name("org.freedesktop.portal.Desktop")
            .unwrap();
        if config.remote_desktop {
            builder = builder
                .serve_at(path, RemoteDesktop(shared.clone()))
                .unwrap();
        }
        if config.clipboard {
            builder = builder
                .serve_at(path, ClipboardPortal(shared.clone()))
                .unwrap();
        }
        if config.registry {
            builder = builder.serve_at(path, Registry(calls.clone())).unwrap();
        }
        if let Some(ic) = config.input_capture {
            builder = builder.serve_at(path, InputCapture(shared, ic)).unwrap();
        }
        let conn = FAKES.block_on(builder.build()).unwrap();
        Portals {
            conn,
            calls,
            shared: shared_copy,
        }
    }

    /// The `n`th libei connection handed out, once it exists.
    pub fn eis(&self, n: usize) -> Arc<FakeEis> {
        crate::daemon::wait_until("a libei connection", || {
            self.shared.eis.lock().unwrap().len() > n
        });
        self.shared.eis.lock().unwrap()[n].clone()
    }

    /// What other applications have on the clipboard.
    pub fn set_contents(&self, contents: &[(&str, &[u8])]) {
        *self.shared.contents.lock().unwrap() = contents
            .iter()
            .map(|(m, d)| (m.to_string(), d.to_vec()))
            .collect();
    }

    /// The portal announces a new selection owner.
    pub fn owner_changed(&self, mime_types: &[&str], session_is_owner: bool) {
        let session = self.shared.sessions.lock().unwrap().last().unwrap().clone();
        let options = HashMap::from([
            (
                "mime_types".to_string(),
                ov(mime_types
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<String>>()),
            ),
            ("session_is_owner".to_string(), ov(session_is_owner)),
        ]);
        FAKES
            .block_on(self.conn.emit_signal(
                None::<&str>,
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Clipboard",
                "SelectionOwnerChanged",
                &(ObjectPath::try_from(session).unwrap(), options),
            ))
            .unwrap();
    }

    /// An application pastes `mime`: the portal asks the session for the data.
    pub fn transfer(&self, mime: &str, serial: u32) {
        let session = self.shared.sessions.lock().unwrap().last().unwrap().clone();
        FAKES
            .block_on(self.conn.emit_signal(
                None::<&str>,
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.Clipboard",
                "SelectionTransfer",
                &(ObjectPath::try_from(session).unwrap(), mime, serial),
            ))
            .unwrap();
    }

    /// What the session wrote for `serial`, once it reported being done.
    pub fn written(&self, serial: u32) -> Vec<u8> {
        use std::io::Read;
        crate::daemon::wait_until("the transfer to finish", || {
            self.calls
                .all()
                .iter()
                .any(|c| c.starts_with(&format!("clipboard.write_done {serial} ")))
        });
        let mut data = Vec::new();
        self.shared
            .writes
            .lock()
            .unwrap()
            .remove(&serial)
            .unwrap()
            .read_to_end(&mut data)
            .unwrap();
        data
    }

    /// The portal tells the capture it was activated at `at` through `barrier`.
    pub fn activate(&self, activation_id: u32, at: (f32, f32), barrier: u32) {
        let session = self.shared.sessions.lock().unwrap().last().unwrap().clone();
        let options = HashMap::from([
            ("activation_id".to_string(), ov(activation_id)),
            ("cursor_position".to_string(), ov(at)),
            ("barrier_id".to_string(), ov(barrier)),
        ]);
        FAKES
            .block_on(self.conn.emit_signal(
                None::<&str>,
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.InputCapture",
                "Activated",
                &(ObjectPath::try_from(session).unwrap(), options),
            ))
            .unwrap();
    }

    pub fn deactivate(&self, activation_id: u32) {
        let session = self.shared.sessions.lock().unwrap().last().unwrap().clone();
        let options = HashMap::from([("activation_id".to_string(), ov(activation_id))]);
        FAKES
            .block_on(self.conn.emit_signal(
                None::<&str>,
                "/org/freedesktop/portal/desktop",
                "org.freedesktop.portal.InputCapture",
                "Deactivated",
                &(ObjectPath::try_from(session).unwrap(), options),
            ))
            .unwrap();
    }
}
