//! Probe 4 (FINDINGS-windows-linux): do the Wayland portals Mouser needs start,
//! persist and carry a clipboard on this desktop? Prints `PROBE key=value` lines.
//!
//! usage: spike_portals info
//!        spike_portals rd [--register] [--token FILE] [--clipboard] [--hold SECS] [--timeout SECS]
//!        spike_portals ic [--register] [--token FILE] [--clipboard] [--hold SECS] [--timeout SECS] [--release-after]

use std::{future::Future, os::fd::AsRawFd, time::Duration};

use ashpd::{
    AppID,
    desktop::{
        PersistMode, Session,
        clipboard::{Clipboard, RequestClipboardOptions, SetSelectionOptions},
        input_capture::{
            Barrier, BarrierID, Capabilities, CreateSession2Options, InputCapture, ReleaseOptions,
            StartOptions as IcStart,
        },
        remote_desktop::{
            DeviceType, RemoteDesktop, SelectDevicesOptions,
            StartOptions as RdStart,
        },
    },
};
use futures_util::StreamExt;

struct Args {
    register: bool,
    token: Option<String>,
    clipboard: bool,
    hold: u64,
    timeout: u64,
    release_after: bool,
    inject_keys: Vec<u32>,
    inject_click: Option<(f32, f32)>,
}

fn parse(rest: &[String]) -> Args {
    let mut a = Args { register: false, token: None, clipboard: false, hold: 0, timeout: 45, release_after: false, inject_keys: vec![], inject_click: None };
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--register" => a.register = true,
            "--clipboard" => a.clipboard = true,
            "--release-after" => a.release_after = true,
            "--token" => { i += 1; a.token = Some(rest[i].clone()) }
            "--inject-keys" => { i += 1; a.inject_keys = rest[i].split(',').map(|k| k.parse().unwrap()).collect() }
            "--inject-click" => {
                i += 1;
                let (x, y) = rest[i].split_once(',').unwrap();
                a.inject_click = Some((x.parse().unwrap(), y.parse().unwrap()))
            }
            "--hold" => { i += 1; a.hold = rest[i].parse().unwrap() }
            "--timeout" => { i += 1; a.timeout = rest[i].parse().unwrap() }
            other => panic!("unknown arg {other}"),
        }
        i += 1;
    }
    a
}

fn p(key: &str, val: impl std::fmt::Debug) {
    println!("PROBE {key}={val:?}");
}

async fn within<T>(secs: u64, what: &str, f: impl Future<Output = T>) -> Option<T> {
    match tokio::time::timeout(Duration::from_secs(secs), f).await {
        Ok(v) => Some(v),
        Err(_) => {
            p(&format!("{what}.result"), "TIMEOUT (no response: a dialog is probably still pending)");
            None
        }
    }
}

fn read_token(a: &Args) -> Option<String> {
    let t = std::fs::read_to_string(a.token.as_ref()?).ok()?;
    let t = t.trim().to_string();
    p("token.loaded", !t.is_empty());
    if t.is_empty() { None } else { Some(t) }
}

fn write_token(a: &Args, t: Option<&str>) {
    p("token.returned", t.is_some());
    if let (Some(path), Some(t)) = (a.token.as_ref(), t) {
        std::fs::write(path, t).unwrap();
    }
}

async fn register(a: &Args) {
    if a.register {
        let r = ashpd::register_host_app(AppID::try_from("com.mouser.app").unwrap()).await;
        p("registry.register", r.map(|_| "ok"));
    }
}

async fn info() {
    let rd = RemoteDesktop::new().await;
    match &rd {
        Ok(rd) => {
            p("remote_desktop.version", rd.version());
            p("remote_desktop.available_device_types", rd.available_device_types().await);
        }
        Err(e) => p("remote_desktop.error", e.to_string()),
    }
    match InputCapture::new().await {
        Ok(ic) => {
            p("input_capture.version", ic.version());
            p("input_capture.supported_capabilities", ic.supported_capabilities().await);
            p("input_capture.supported_capabilities_raw", ic.get_property::<u32>("SupportedCapabilities").await);
        }
        Err(e) => p("input_capture.error", e.to_string()),
    }
    match Clipboard::new().await {
        Ok(c) => p("clipboard.version", c.version()),
        Err(e) => p("clipboard.error", e.to_string()),
    }
}

async fn rd(a: Args) {
    register(&a).await;
    let rd = RemoteDesktop::new().await.unwrap();
    let session = rd.create_session(Default::default()).await.unwrap();
    p("rd.session", "created");
    let clip = Clipboard::new().await.ok();
    if a.clipboard {
        let r = clip.as_ref().unwrap().request(&session, RequestClipboardOptions::default()).await;
        p("rd.clipboard.request", r.map(|_| "ok"));
    }
    let token = read_token(&a);
    rd.select_devices(
        &session,
        SelectDevicesOptions::default()
            .set_devices(DeviceType::Keyboard | DeviceType::Pointer)
            .set_persist_mode(PersistMode::ExplicitlyRevoked)
            .set_restore_token(token.as_deref()),
    )
    .await
    .unwrap()
    .response()
    .unwrap();
    p("rd.select_devices", "ok");
    let t0 = std::time::Instant::now();
    let Some(req) = within(a.timeout, "rd.start", rd.start(&session, None, RdStart::default())).await else {
        return;
    };
    match req.and_then(|r| r.response()) {
        Ok(sel) => {
            p("rd.start.secs", t0.elapsed().as_secs_f32());
            p("rd.start.devices", sel.devices());
            p("rd.start.clipboard_enabled", sel.is_clipboard_enabled());
            write_token(&a, sel.restore_token());
        }
        Err(e) => {
            p("rd.start.result", format!("ERR {e}"));
            return;
        }
    }
    let fd = rd.connect_to_eis(&session, Default::default()).await;
    p("rd.connect_to_eis", fd.as_ref().map(|f| f.as_raw_fd() >= 0));
    if let Ok(fd) = fd {
        let (keys, click) = (a.inject_keys.clone(), a.inject_click);
        if !keys.is_empty() || click.is_some() {
            std::thread::spawn(move || {
                tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(eis_inject(fd.into(), keys, click));
            });
        }
    }
    let r = rd.notify_pointer_motion(&session, 1.0, 1.0, Default::default()).await;
    p("rd.notify_pointer_motion", r.map(|_| "ok"));
    if a.clipboard {
        clipboard_roundtrip(clip.as_ref().unwrap(), &session, "rd").await;
    }
    tokio::time::sleep(Duration::from_secs(a.hold)).await;
}

async fn clipboard_roundtrip<T: ashpd::desktop::clipboard::IsClipboardSession>(c: &Clipboard, session: &Session<T>, tag: &str) {
    let mut owner = Box::pin(c.receive_selection_owner_changed::<T>().await.unwrap());
    let mut transfer = Box::pin(c.receive_selection_transfer::<T>().await.unwrap());
    let r = c.set_selection(session, SetSelectionOptions::default().set_mime_types(&["text/plain;charset=utf-8"])).await;
    p(&format!("{tag}.clipboard.set_selection"), r.map(|_| "ok"));
    // Serve one transfer request, if the compositor asks (it does when another client pastes).
    let serve = async {
        while let Some((_s, mime, serial)) = transfer.next().await {
            p(&format!("{tag}.clipboard.transfer_requested"), mime);
            let fd = c.selection_write(session, serial).await.unwrap();
            let mut f = std::fs::File::from(std::os::fd::OwnedFd::from(fd));
            std::io::Write::write_all(&mut f, b"mouser-probe-clipboard").unwrap();
            drop(f);
            c.selection_write_done(session, serial, true).await.unwrap();
            p(&format!("{tag}.clipboard.transfer_served"), true);
        }
    };
    let raw = async {
        let mut sig = c.receive_signal("SelectionOwnerChanged").await.unwrap();
        while let Some(m) = sig.next().await {
            p(&format!("{tag}.clipboard.raw_owner_changed_body_signature"), m.body().signature().to_string());
        }
    };
    let watch = async {
        while let Some((_s, o)) = owner.next().await {
            p(&format!("{tag}.clipboard.owner_changed"), (o.session_is_owner(), o.mime_types().to_vec()));
        }
    };
    let _ = tokio::time::timeout(Duration::from_secs(20), async { tokio::join!(serve, watch, raw) }).await;
}

/// Act as an EIS sender: type `keys` (evdev codes) and click at `click` once the devices resume.
async fn eis_inject(fd: std::os::fd::OwnedFd, keys: Vec<u32>, click: Option<(f32, f32)>) {
    use reis::{ei, event::{DeviceCapability, EiEvent}};
    let ctx = ei::Context::new(std::os::unix::net::UnixStream::from(fd)).unwrap();
    let (conn, mut events) = ctx.handshake_tokio("mouser-probe-inject", ei::handshake::ContextType::Sender).await.unwrap();
    p("inject.handshake", "ok");
    let mut t = 0u64;
    while let Some(ev) = events.next().await {
        match ev.unwrap() {
            EiEvent::SeatAdded(e) => {
                e.seat.bind_capabilities(DeviceCapability::Pointer | DeviceCapability::PointerAbsolute | DeviceCapability::Keyboard | DeviceCapability::Button | DeviceCapability::Scroll);
                let _ = ctx.flush();
            }
            EiEvent::DeviceAdded(e) => p("inject.device_added", (e.device.name().map(str::to_owned), e.device.device_type(), e.device.regions().len())),
            EiEvent::DeviceResumed(e) => {
                let d = e.device.device();
                p("inject.device_resumed", e.device.name().map(str::to_owned));
                d.start_emulating(conn.serial(), 1);
                if let Some(kb) = e.device.interface::<ei::Keyboard>() {
                    for k in &keys {
                        for st in [ei::keyboard::KeyState::Press, ei::keyboard::KeyState::Released] {
                            kb.key(*k, st);
                            t += 20_000;
                            d.frame(conn.serial(), t);
                            let _ = ctx.flush();
                            tokio::time::sleep(Duration::from_millis(60)).await;
                        }
                    }
                    p("inject.keys_sent", keys.len());
                }
                if let (Some(pa), Some((x, y))) = (e.device.interface::<ei::PointerAbsolute>(), click) {
                    pa.motion_absolute(x, y);
                    t += 20_000;
                    d.frame(conn.serial(), t);
                    let _ = ctx.flush();
                    if let Some(b) = e.device.interface::<ei::Button>() {
                        for st in [ei::button::ButtonState::Press, ei::button::ButtonState::Released] {
                            b.button(0x110, st);
                            t += 20_000;
                            d.frame(conn.serial(), t);
                            let _ = ctx.flush();
                            tokio::time::sleep(Duration::from_millis(60)).await;
                        }
                    }
                    p("inject.click_sent", (x, y));
                }
                d.stop_emulating(conn.serial());
                let _ = ctx.flush();
            }
            _ => {}
        }
    }
}

/// `eisfd FD ACTION...`: act as an EIS sender on an already-open EIS socket (inherited fd) and run a script.
/// Actions: `abs:X,Y`  `rel:DX,DY`  `click`  `btndown`  `btnup`  `key:CODE`  `sleep:SECS`.
/// Used to drive compositors that expose their own EIS (KWin: org.kde.KWin.EIS.RemoteDesktop).
async fn eis_script(fd: std::os::fd::OwnedFd, actions: Vec<String>) {
    use reis::{ei, event::{DeviceCapability, EiEvent}};
    let ctx = ei::Context::new(std::os::unix::net::UnixStream::from(fd)).unwrap();
    let (conn, mut events) = ctx.handshake_tokio("mouser-probe-script", ei::handshake::ContextType::Sender).await.unwrap();
    let mut devices = Vec::new();
    loop {
        match tokio::time::timeout(Duration::from_millis(1500), events.next()).await {
            Ok(Some(ev)) => match ev.unwrap() {
                EiEvent::SeatAdded(e) => {
                    e.seat.bind_capabilities(DeviceCapability::Pointer | DeviceCapability::PointerAbsolute | DeviceCapability::Keyboard | DeviceCapability::Button | DeviceCapability::Scroll);
                    let _ = ctx.flush();
                }
                EiEvent::DeviceAdded(e) => p("script.device_added", (e.device.name().map(str::to_owned), e.device.regions().len())),
                EiEvent::DeviceResumed(e) => devices.push(e.device),
                _ => {}
            },
            Ok(None) => return,
            Err(_) if !devices.is_empty() => break,
            Err(_) => {}
        }
    }
    let mut t = 0u64;
    macro_rules! frame {
        ($d:expr) => {{
            t += 20_000;
            $d.device().frame(conn.serial(), t);
            let _ = ctx.flush();
            tokio::time::sleep(Duration::from_millis(80)).await;
        }};
    }
    for d in &devices {
        d.device().start_emulating(conn.serial(), 1);
    }
    for a in &actions {
        let (name, arg) = a.split_once(':').unwrap_or((a.as_str(), ""));
        let xy = || {
            let (x, y) = arg.split_once(',').unwrap();
            (x.parse::<f32>().unwrap(), y.parse::<f32>().unwrap())
        };
        match name {
            "abs" => {
                let (x, y) = xy();
                let d = devices.iter().find(|d| d.interface::<ei::PointerAbsolute>().is_some()).expect("no abs pointer device");
                d.interface::<ei::PointerAbsolute>().unwrap().motion_absolute(x, y);
                frame!(d);
            }
            "rel" => {
                let (x, y) = xy();
                let d = devices.iter().find(|d| d.interface::<ei::Pointer>().is_some()).expect("no rel pointer device");
                d.interface::<ei::Pointer>().unwrap().motion_relative(x, y);
                frame!(d);
            }
            "click" | "btndown" | "btnup" => {
                let d = devices.iter().find(|d| d.interface::<ei::Button>().is_some()).expect("no button device");
                let b = d.interface::<ei::Button>().unwrap();
                if name != "btnup" {
                    b.button(0x110, ei::button::ButtonState::Press);
                    frame!(d);
                }
                if name != "btndown" {
                    b.button(0x110, ei::button::ButtonState::Released);
                    frame!(d);
                }
            }
            "key" => {
                let d = devices.iter().find(|d| d.interface::<ei::Keyboard>().is_some()).expect("no keyboard device");
                let k = d.interface::<ei::Keyboard>().unwrap();
                k.key(arg.parse().unwrap(), ei::keyboard::KeyState::Press);
                frame!(d);
                k.key(arg.parse().unwrap(), ei::keyboard::KeyState::Released);
                frame!(d);
            }
            "sleep" => tokio::time::sleep(Duration::from_secs_f32(arg.parse().unwrap())).await,
            other => panic!("bad action {other}"),
        }
    }
    p("script.done", actions.len());
}

/// Read the EIS stream the portal hands out and print what libei delivers while captured.
async fn eis_reader(fd: std::os::fd::OwnedFd) {
    use reis::{ei, event::{DeviceCapability, EiEvent}};
    let ctx = ei::Context::new(std::os::unix::net::UnixStream::from(fd)).unwrap();
    let (_conn, mut events) = ctx.handshake_tokio("mouser-probe", ei::handshake::ContextType::Receiver).await.unwrap();
    p("eis.handshake", "ok");
    let (mut motion, mut button, mut key, mut scroll, mut last_other) = (0u32, 0u32, 0u32, 0u32, 0u32);
    while let Some(ev) = events.next().await {
        let ev = ev.unwrap();
        match &ev {
            EiEvent::SeatAdded(e) => {
                e.seat.bind_capabilities(DeviceCapability::Pointer | DeviceCapability::PointerAbsolute | DeviceCapability::Keyboard | DeviceCapability::Scroll | DeviceCapability::Button);
                let _ = ctx.flush();
            }
            EiEvent::DeviceAdded(e) => p("eis.device_added", (e.device.name(), e.device.device_type())),
            EiEvent::PointerMotion(e) => {
                motion += 1;
                if motion <= 3 { p("eis.pointer_motion", (e.dx, e.dy)); }
            }
            EiEvent::Button(e) => { button += 1; p("eis.button", (e.button, e.state)); }
            EiEvent::KeyboardKey(e) => { key += 1; p("eis.key", (e.key, e.state)); }
            EiEvent::ScrollDelta(_) | EiEvent::ScrollDiscrete(_) => { scroll += 1; }
            _ => {}
        }
        if let EiEvent::Frame(_) = ev {
            let other = button + key + scroll;
            if other != last_other {
                last_other = other;
                println!("PROBE eis.totals=motion:{motion} button:{button} key:{key} scroll:{scroll}");
            }
        }
    }
}

async fn ic(a: Args) {
    register(&a).await;
    let ic = InputCapture::new().await.unwrap();
    p("ic.version", ic.version());
    let clip = Clipboard::new().await.ok();
    let token = read_token(&a);
    let t0 = std::time::Instant::now();
    let caps = Capabilities::Keyboard | Capabilities::Pointer;
    let session = match ic.create_session2(CreateSession2Options::default()).await {
        Ok(session) => {
            p("ic.create_session2", "ok (interface v2: explicit Start, persistence available)");
            if a.clipboard {
                let r = clip.as_ref().unwrap().request(&session, RequestClipboardOptions::default()).await;
                p("ic.clipboard.request", r.map(|_| "ok"));
            }
            let opts = IcStart::default()
                .set_capabilities(caps)
                .set_persist_mode(PersistMode::ExplicitlyRevoked)
                .set_restore_token(token);
            let Some(req) = within(a.timeout, "ic.start", ic.start(&session, None, opts)).await else {
                return;
            };
            match req.and_then(|r| r.response()) {
                Ok(st) => {
                    p("ic.start.secs", t0.elapsed().as_secs_f32());
                    p("ic.start.capabilities", st.capabilities());
                    p("ic.start.clipboard_enabled", st.is_clipboard_enabled());
                    write_token(&a, st.restore_token());
                }
                Err(e) => {
                    p("ic.start.result", format!("ERR {e}"));
                    return;
                }
            }
            session
        }
        Err(ashpd::Error::RequiresVersion(want, have)) => {
            p("ic.create_session2", format!("unavailable: needs interface v{want}, portal has v{have}; using v1 CreateSession (no persistence, no clipboard)"));
            let opts = ashpd::desktop::input_capture::CreateSessionOptions::default().set_capabilities(caps);
            let Some(r) = within(a.timeout, "ic.create_session", ic.create_session(None, opts)).await else {
                return;
            };
            match r {
                Ok((s, granted)) => {
                    p("ic.create_session.secs", t0.elapsed().as_secs_f32());
                    p("ic.create_session.capabilities", granted);
                    if a.clipboard {
                        // v1 has no separate Start, so a clipboard request can only come too late.
                        let r = clip.as_ref().unwrap().request(&s, RequestClipboardOptions::default()).await;
                        p("ic.clipboard.request_after_create", r.map(|_| "ok"));
                    }
                    s
                }
                Err(e) => {
                    p("ic.create_session.result", format!("ERR {e}"));
                    return;
                }
            }
        }
        Err(e) => {
            p("ic.create_session2", format!("ERR {e}"));
            return;
        }
    };
    let zones = ic.zones(&session, Default::default()).await.unwrap().response().unwrap();
    for r in zones.regions() {
        p("ic.zone", (r.x_offset(), r.y_offset(), r.width(), r.height()));
    }
    // Right-edge barrier only (the probe moves the pointer right). Which coordinates does the
    // compositor accept? Try the zone edge (x+w) and the last pixel column (x+w-1), with both
    // inclusive and exclusive y extents, and report each outcome.
    let r = &zones.regions()[0];
    let (x0, y0, w, h) = (r.x_offset(), r.y_offset(), r.width() as i32, r.height() as i32);
    let candidates = [(x0 + w, y0, x0 + w, y0 + h - 1), (x0 + w, y0, x0 + w, y0 + h), (x0 + w - 1, y0, x0 + w - 1, y0 + h - 1)];
    let mut accepted = false;
    for (i, c) in candidates.into_iter().enumerate() {
        let barrier = Barrier::new(BarrierID::new(i as u32 + 1).unwrap(), c);
        let failed = ic.set_pointer_barriers(&session, &[barrier], zones.zone_set(), Default::default()).await.unwrap().response().unwrap();
        p(&format!("ic.barrier{c:?}.failed"), failed.failed_barriers().to_vec());
        if failed.failed_barriers().is_empty() {
            accepted = true;
            break;
        }
    }
    p("ic.barriers.accepted", accepted);
    let eis = ic.connect_to_eis(&session, Default::default()).await.unwrap();
    p("ic.connect_to_eis", eis.as_raw_fd() >= 0);
    // reis 0.7.1's event stream is !Send: give it its own thread with a current-thread runtime.
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(eis_reader(eis));
    });
    let mut activated = Box::pin(ic.receive_activated().await.unwrap());
    let mut deactivated = Box::pin(ic.receive_deactivated().await.unwrap());
    ic.enable(&session, Default::default()).await.unwrap();
    p("ic.enable", "ok");
    println!("PROBE ic.waiting=true");
    let deadline = Duration::from_secs(a.hold.max(1));
    let _ = tokio::time::timeout(deadline, async {
        loop {
            tokio::select! {
                Some(act) = activated.next() => {
                    p("ic.activated", (act.activation_id(), act.cursor_position(), act.barrier_id()));
                    if a.clipboard {
                        clipboard_roundtrip(clip.as_ref().unwrap(), &session, "ic").await;
                    }
                    if a.release_after {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        let r = ic.release(&session, ReleaseOptions::default().set_activation_id(act.activation_id())).await;
                        p("ic.release", r.map(|_| "ok"));
                    }
                }
                Some(d) = deactivated.next() => p("ic.deactivated", d.activation_id()),
            }
        }
    })
    .await;
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("info") => info().await,
        Some("eisfd") => {
            use std::os::fd::FromRawFd;
            // SAFETY: the launcher (scripts/kwin_eis.py) passes an open EIS socket fd we now own.
            let fd = unsafe { std::os::fd::OwnedFd::from_raw_fd(args[1].parse().unwrap()) };
            eis_script(fd, args[2..].to_vec()).await
        }
        Some("rd") => rd(parse(&args[1..])).await,
        Some("ic") => ic(parse(&args[1..])).await,
        _ => panic!("usage: spike_portals info|rd|ic"),
    }
}
