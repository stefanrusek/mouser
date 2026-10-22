//! `VirtualPlatform`: the sanctioned in-process OS stand-in (decision D2).
//!
//! Implements only the `mouser-platform` traits. Cloning gives another handle
//! to the same virtual computer: the core owns one as `Arc<dyn Platform>` while
//! the test keeps another to script inputs and read what was injected.

use crate::report;
use mouser_platform::{
    Autostart, Capability, ClipRep, Clipboard, DropTarget, Error, FileDrag, InputCapture,
    InputInject, InterfaceKind, Limitation, LocalDisplay, LocalInput, NetInterface, Os,
    PermissionKind, PermissionState, Platform, PlatformInfo, Result, SecretStore,
    SecureInputHolder, SessionType, WatchHandle,
};
use std::collections::{BTreeMap, BTreeSet};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, MutexGuard};

/// One thing the core asked the virtual OS to inject, in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Injected {
    MotionAbs { x: f64, y: f64 },
    Button { button: u8, down: bool },
    Scroll { dx: f64, dy: f64 },
    Key { usage: u32, down: bool },
    SecureAttention,
}

#[derive(Default)]
struct Capture {
    tx: Option<Sender<LocalInput>>,
    grabbed: bool,
    warps: Vec<(f64, f64)>,
}

struct State {
    info: PlatformInfo,
    capture_supported: bool,
    inject_supported: bool,
    clipboard_supported: bool,
    file_drag_supported: bool,
    secure_attention_supported: bool,

    displays: Vec<LocalDisplay>,
    display_watchers: Watchers<Vec<LocalDisplay>>,
    permissions: BTreeMap<PermissionKind, PermissionState>,
    opened_settings: Vec<PermissionKind>,

    capture: Capture,
    delivered_locally: Vec<LocalInput>,
    injected: Vec<Injected>,

    clipboard: Vec<ClipRep>,
    clipboard_count: u64,

    interfaces: Vec<NetInterface>,
    network_watchers: Watchers<()>,
    problem_watchers: Watchers<String>,
    next_watch: u64,

    fullscreen: bool,
    elevated: bool,
    secure_input: Option<SecureInputHolder>,
    secrets: BTreeMap<String, Vec<u8>>,
    autostart: bool,

    outgoing_drag: Option<Vec<PathBuf>>,
    drop_target: PathBuf,
    incoming_drags: Vec<Vec<String>>,
}

type Watchers<T> = Vec<(u64, Sender<T>)>;

/// A virtual computer. See the module documentation.
#[derive(Clone)]
pub struct VirtualPlatform {
    state: Arc<Mutex<State>>,
}

fn limitation(capability: Capability, text: &str) -> Limitation {
    Limitation {
        capability,
        text: text.to_string(),
    }
}

fn default_display() -> LocalDisplay {
    LocalDisplay {
        key: "virtual-0".into(),
        name: "Virtual Display".into(),
        pixel_size: (1920, 1080),
        logical_size: (1920.0, 1080.0),
        scale: 1.0,
        physical_size_mm: Some((527.0, 296.0)),
        origin: (0.0, 0.0),
    }
}

fn default_interfaces() -> Vec<NetInterface> {
    vec![
        NetInterface {
            name: "eth0".into(),
            kind: InterfaceKind::PhysicalLan,
            addrs: vec![IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10))],
        },
        NetInterface {
            name: "lo".into(),
            kind: InterfaceKind::Loopback,
            addrs: vec![IpAddr::V4(Ipv4Addr::LOCALHOST)],
        },
    ]
}

const NO_SECURE_ATTENTION: &str = "Ctrl-Alt-Del cannot be sent to this computer.";

impl VirtualPlatform {
    /// A Linux/X11-like computer where everything works except Ctrl-Alt-Del:
    /// one 1920x1080 display, interfaces `eth0` (physical LAN) and `lo`, no
    /// permissions, empty clipboard, file drag available.
    pub fn new() -> Self {
        report::note("VirtualPlatform");
        let state = State {
            info: PlatformInfo {
                os: Os::Linux,
                version: "virtual".into(),
                session: SessionType::X11,
                limitations: vec![limitation(Capability::SecureAttention, NO_SECURE_ATTENTION)],
            },
            capture_supported: true,
            inject_supported: true,
            clipboard_supported: true,
            file_drag_supported: true,
            secure_attention_supported: false,
            displays: vec![default_display()],
            display_watchers: Vec::new(),
            permissions: BTreeMap::new(),
            opened_settings: Vec::new(),
            capture: Capture::default(),
            delivered_locally: Vec::new(),
            injected: Vec::new(),
            clipboard: Vec::new(),
            clipboard_count: 0,
            interfaces: default_interfaces(),
            network_watchers: Vec::new(),
            problem_watchers: Vec::new(),
            next_watch: 0,
            fullscreen: false,
            elevated: false,
            secure_input: None,
            secrets: BTreeMap::new(),
            autostart: false,
            outgoing_drag: None,
            drop_target: PathBuf::from("/virtual/drop"),
            incoming_drags: Vec::new(),
        };
        VirtualPlatform {
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// A platform that supports nothing optional: capture, inject, clipboard and
    /// file drag all return `Unsupported` (`None` for file drag) and each is
    /// listed in `info().limitations`; its display has no known physical size.
    pub fn limited() -> Self {
        let platform = Self::new();
        {
            let mut s = platform.lock();
            s.capture_supported = false;
            s.inject_supported = false;
            s.displays[0].physical_size_mm = None;
            s.clipboard_supported = false;
            s.file_drag_supported = false;
            s.info.limitations = vec![
                limitation(
                    Capability::Capture,
                    "Mouser cannot watch this computer's input.",
                ),
                limitation(
                    Capability::Inject,
                    "Mouser cannot control this computer's input.",
                ),
                limitation(Capability::Clipboard, "The clipboard cannot be shared."),
                limitation(
                    Capability::FileDrag,
                    "Files cannot be dragged to or from this computer.",
                ),
            ];
        }
        platform
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }

    // ---- scripting: platform description ----

    /// Replace the reported `PlatformInfo` wholesale.
    pub fn set_info(&self, info: PlatformInfo) {
        self.lock().info = info;
    }

    /// Make `InputInject::secure_attention` succeed (recording
    /// `Injected::SecureAttention`) and drop its limitation, as on Windows.
    pub fn set_secure_attention_supported(&self, supported: bool) {
        let mut s = self.lock();
        s.secure_attention_supported = supported;
        s.info
            .limitations
            .retain(|l| l.capability != Capability::SecureAttention);
        if !supported {
            s.info
                .limitations
                .push(limitation(Capability::SecureAttention, NO_SECURE_ATTENTION));
        }
    }

    /// Replace the displays and notify every `watch_displays` subscriber.
    pub fn set_displays(&self, displays: Vec<LocalDisplay>) {
        let mut s = self.lock();
        s.displays = displays.clone();
        for (_, tx) in &s.display_watchers {
            let _ = tx.send(displays.clone());
        }
    }

    pub fn set_permission(&self, kind: PermissionKind, state: PermissionState) {
        self.lock().permissions.insert(kind, state);
    }

    /// Kinds passed to `open_permission_settings`, in order.
    pub fn opened_permission_settings(&self) -> Vec<PermissionKind> {
        self.lock().opened_settings.clone()
    }

    pub fn set_fullscreen(&self, fullscreen: bool) {
        self.lock().fullscreen = fullscreen;
    }

    pub fn set_foreground_elevated(&self, elevated: bool) {
        self.lock().elevated = elevated;
    }

    pub fn set_secure_input(&self, holder: Option<SecureInputHolder>) {
        self.lock().secure_input = holder;
    }

    // ---- scripting: network ----

    /// Replace the interface list and fire every `watch_network` subscriber.
    pub fn set_interfaces(&self, interfaces: Vec<NetInterface>) {
        self.lock().interfaces = interfaces;
        self.trigger_network_change();
    }

    /// Fire every `watch_network` subscriber without changing anything.
    pub fn trigger_network_change(&self) {
        for (_, tx) in &self.lock().network_watchers {
            let _ = tx.send(());
        }
    }

    /// Tell every `watch_problems` subscriber about a problem the platform
    /// noticed by itself.
    pub fn report_problem(&self, text: &str) {
        for (_, tx) in &self.lock().problem_watchers {
            let _ = tx.send(text.to_string());
        }
    }

    // ---- scripting: input ----

    /// Simulate a local hardware input event. Delivered to the capture channel
    /// when capturing; reaches local apps (and is recorded in
    /// [`local_delivered`](Self::local_delivered)) unless grabbed. Returns
    /// whether local apps received it.
    pub fn send_input(&self, input: LocalInput) -> bool {
        let mut s = self.lock();
        if let Some(tx) = &s.capture.tx {
            let _ = tx.send(input);
        }
        if s.capture.grabbed {
            return false;
        }
        s.delivered_locally.push(input);
        true
    }

    /// Local events that reached local apps (were not swallowed by a grab).
    pub fn local_delivered(&self) -> Vec<LocalInput> {
        self.lock().delivered_locally.clone()
    }

    pub fn is_capturing(&self) -> bool {
        self.lock().capture.tx.is_some()
    }

    pub fn is_grabbed(&self) -> bool {
        self.lock().capture.grabbed
    }

    /// Positions passed to `InputCapture::warp`, in order.
    pub fn warps(&self) -> Vec<(f64, f64)> {
        self.lock().capture.warps.clone()
    }

    /// Everything injected so far, in order (key/button releases on injector
    /// drop included).
    pub fn injected(&self) -> Vec<Injected> {
        self.lock().injected.clone()
    }

    /// Return and clear the injection record.
    pub fn take_injected(&self) -> Vec<Injected> {
        std::mem::take(&mut self.lock().injected)
    }

    // ---- scripting: clipboard, secrets, autostart, drag ----

    /// Simulate the user copying on this computer: replaces the contents and
    /// advances the change counter.
    pub fn set_local_clipboard(&self, reps: Vec<ClipRep>) {
        let mut s = self.lock();
        s.clipboard = reps;
        s.clipboard_count += 1;
    }

    pub fn clipboard_contents(&self) -> Vec<ClipRep> {
        self.lock().clipboard.clone()
    }

    pub fn is_autostart_enabled(&self) -> bool {
        self.lock().autostart
    }

    /// Files "being dragged" at the display edge; returned (once) by
    /// `FileDrag::outgoing`.
    pub fn script_outgoing_drag(&self, paths: Option<Vec<PathBuf>>) {
        self.lock().outgoing_drag = paths;
    }

    /// Where `FileDrag::begin_incoming` reports the drop landed.
    pub fn script_drop_target(&self, dir: PathBuf) {
        self.lock().drop_target = dir;
    }

    /// File-name lists passed to `FileDrag::begin_incoming`, in order.
    pub fn incoming_drags(&self) -> Vec<Vec<String>> {
        self.lock().incoming_drags.clone()
    }

    fn watch<T: Send + 'static>(
        &self,
        select: fn(&mut State) -> &mut Watchers<T>,
        tx: Sender<T>,
    ) -> WatchHandle {
        let mut s = self.lock();
        let id = s.next_watch;
        s.next_watch += 1;
        select(&mut s).push((id, tx));
        let state = self.state.clone();
        WatchHandle::new(move || {
            select(&mut state.lock().unwrap()).retain(|(i, _)| *i != id);
        })
    }
}

impl Default for VirtualPlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for VirtualPlatform {
    fn info(&self) -> PlatformInfo {
        self.lock().info.clone()
    }

    fn displays(&self) -> Result<Vec<LocalDisplay>> {
        Ok(self.lock().displays.clone())
    }

    fn watch_displays(&self, tx: Sender<Vec<LocalDisplay>>) -> Result<WatchHandle> {
        Ok(self.watch(|s| &mut s.display_watchers, tx))
    }

    fn permissions(&self) -> Vec<(PermissionKind, PermissionState)> {
        self.lock()
            .permissions
            .iter()
            .map(|(k, v)| (*k, *v))
            .collect()
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        self.lock().opened_settings.push(kind);
        Ok(())
    }

    fn capture(&self) -> Result<Box<dyn InputCapture>> {
        if !self.lock().capture_supported {
            return Err(Error::Unsupported);
        }
        Ok(Box::new(VirtualCapture(self.clone())))
    }

    fn inject(&self) -> Result<Box<dyn InputInject>> {
        if !self.lock().inject_supported {
            return Err(Error::Unsupported);
        }
        Ok(Box::new(VirtualInject {
            platform: self.clone(),
            keys: BTreeSet::new(),
            buttons: BTreeSet::new(),
        }))
    }

    fn clipboard(&self) -> Result<Box<dyn Clipboard>> {
        if !self.lock().clipboard_supported {
            return Err(Error::Unsupported);
        }
        Ok(Box::new(VirtualClipboard(self.clone())))
    }

    fn secure_input(&self) -> Option<SecureInputHolder> {
        self.lock().secure_input.clone()
    }

    fn foreground_is_fullscreen(&self) -> bool {
        self.lock().fullscreen
    }

    fn interfaces(&self) -> Vec<NetInterface> {
        self.lock().interfaces.clone()
    }

    fn watch_network(&self, tx: Sender<()>) -> Result<WatchHandle> {
        Ok(self.watch(|s| &mut s.network_watchers, tx))
    }

    fn watch_problems(&self, tx: Sender<String>) -> Result<WatchHandle> {
        Ok(self.watch(|s| &mut s.problem_watchers, tx))
    }

    fn foreground_elevated(&self) -> bool {
        self.lock().elevated
    }

    fn secrets(&self) -> Box<dyn SecretStore> {
        Box::new(VirtualSecrets(self.clone()))
    }

    fn autostart(&self) -> Box<dyn Autostart> {
        Box::new(VirtualAutostart(self.clone()))
    }

    fn file_drag(&self) -> Option<Box<dyn FileDrag>> {
        if !self.lock().file_drag_supported {
            return None;
        }
        Some(Box::new(VirtualFileDrag(self.clone())))
    }
}

struct VirtualCapture(VirtualPlatform);

impl InputCapture for VirtualCapture {
    fn start(&mut self, tx: Sender<LocalInput>) -> Result<()> {
        self.0.lock().capture.tx = Some(tx);
        Ok(())
    }

    fn set_grabbed(&mut self, grabbed: bool) -> Result<()> {
        self.0.lock().capture.grabbed = grabbed;
        Ok(())
    }

    fn warp(&mut self, x: f64, y: f64) -> Result<()> {
        self.0.lock().capture.warps.push((x, y));
        Ok(())
    }

    fn stop(&mut self) {
        let mut s = self.0.lock();
        s.capture.tx = None;
        s.capture.grabbed = false;
    }
}

impl Drop for VirtualCapture {
    fn drop(&mut self) {
        self.stop();
    }
}

struct VirtualInject {
    platform: VirtualPlatform,
    keys: BTreeSet<u32>,
    buttons: BTreeSet<u8>,
}

impl VirtualInject {
    fn record(&self, injected: Injected) {
        self.platform.lock().injected.push(injected);
    }
}

impl InputInject for VirtualInject {
    fn motion_abs(&mut self, x: f64, y: f64) -> Result<()> {
        self.record(Injected::MotionAbs { x, y });
        Ok(())
    }

    fn button(&mut self, button: u8, down: bool) -> Result<()> {
        if down {
            self.buttons.insert(button);
        } else {
            self.buttons.remove(&button);
        }
        self.record(Injected::Button { button, down });
        Ok(())
    }

    fn scroll(&mut self, dx: f64, dy: f64) -> Result<()> {
        self.record(Injected::Scroll { dx, dy });
        Ok(())
    }

    fn key(&mut self, usage: u32, down: bool) -> Result<()> {
        if down {
            self.keys.insert(usage);
        } else {
            self.keys.remove(&usage);
        }
        self.record(Injected::Key { usage, down });
        Ok(())
    }

    fn secure_attention(&mut self) -> Result<()> {
        if !self.platform.lock().secure_attention_supported {
            return Err(Error::Unsupported);
        }
        self.record(Injected::SecureAttention);
        Ok(())
    }
}

impl Drop for VirtualInject {
    /// Rule 2: anything still held is released.
    fn drop(&mut self) {
        let mut s = self.platform.lock();
        for button in std::mem::take(&mut self.buttons) {
            s.injected.push(Injected::Button {
                button,
                down: false,
            });
        }
        for usage in std::mem::take(&mut self.keys) {
            s.injected.push(Injected::Key { usage, down: false });
        }
    }
}

struct VirtualClipboard(VirtualPlatform);

impl Clipboard for VirtualClipboard {
    fn read(&mut self) -> Result<Vec<ClipRep>> {
        Ok(self.0.clipboard_contents())
    }

    fn write(&mut self, reps: Vec<ClipRep>) -> Result<()> {
        self.0.set_local_clipboard(reps);
        Ok(())
    }

    fn change_count(&mut self) -> u64 {
        self.0.lock().clipboard_count
    }
}

struct VirtualSecrets(VirtualPlatform);

impl SecretStore for VirtualSecrets {
    fn get(&self, name: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.0.lock().secrets.get(name).cloned())
    }

    fn set(&mut self, name: &str, value: &[u8]) -> Result<()> {
        self.0
            .lock()
            .secrets
            .insert(name.to_string(), value.to_vec());
        Ok(())
    }

    fn delete(&mut self, name: &str) -> Result<()> {
        self.0.lock().secrets.remove(name);
        Ok(())
    }
}

struct VirtualAutostart(VirtualPlatform);

impl Autostart for VirtualAutostart {
    fn enabled(&self) -> bool {
        self.0.is_autostart_enabled()
    }

    fn set(&mut self, on: bool) -> Result<()> {
        self.0.lock().autostart = on;
        Ok(())
    }
}

struct VirtualFileDrag(VirtualPlatform);

impl FileDrag for VirtualFileDrag {
    fn outgoing(&mut self) -> Option<Vec<PathBuf>> {
        self.0.lock().outgoing_drag.take()
    }

    fn begin_incoming(&mut self, names: Vec<String>) -> Result<DropTarget> {
        let mut s = self.0.lock();
        s.incoming_drags.push(names);
        Ok(DropTarget(s.drop_target.clone()))
    }
}
