use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use mouser_core::release::{Bundle, minisign};
use mouser_platform::Os;
use mouser_proto::control::{RpcError, UpdateResult};

use crate::support_tests::{FakeTray, fixture_offline, fixture_releasing_at, status_of};
use crate::tray::{Action, Appearance, Entry, OPEN_ID, TrayController, UPDATE_ID, action, view};
use crate::updater::{Applier, Key, Server, TARGET, apply, check, installer, line, poll, show};

fn update(available: Option<&str>) -> UpdateResult {
    UpdateResult {
        current: "1.2.3".into(),
        available: available.map(Into::into),
    }
}

fn bundle() -> Bundle {
    Bundle {
        version: "1.3.0".into(),
        bytes: b"the new Mouser".to_vec(),
    }
}

/// `GET path` from a loopback server: the status line and the body.
fn get(endpoint: &str, path: &str) -> (String, Vec<u8>) {
    let port = endpoint
        .strip_prefix("http://127.0.0.1:")
        .unwrap()
        .split('/')
        .next()
        .unwrap();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream
        .write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
        .unwrap();
    let mut reply = Vec::new();
    stream.read_to_end(&mut reply).unwrap();
    let split = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(reply[..split].to_vec()).unwrap();
    (
        head.lines().next().unwrap().to_string(),
        reply[split + 4..].to_vec(),
    )
}

#[test]
fn the_tray_says_the_version_and_offers_the_update_when_there_is_one() {
    assert_eq!(line(&update(None)), (None, "Version 1.2.3".to_string()));
    assert_eq!(
        line(&update(Some("1.3.0"))),
        (
            Some(UPDATE_ID),
            "Version 1.2.3 \u{2014} Update to 1.3.0 available".to_string()
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn the_version_line_comes_right_after_the_summary() {
    let f = fixture_offline().await;
    let status = status_of(&f.host).await;
    let plain = view(&status, Os::MacOs, Appearance::Light, None);
    let with = view(
        &status,
        Os::MacOs,
        Appearance::Light,
        Some(&update(Some("1.3.0"))),
    );
    assert_eq!(with.entries.len(), plain.entries.len() + 1);
    assert_eq!(with.entries[0], plain.entries[0]);
    assert_eq!(
        with.entries[1],
        Entry {
            id: Some(UPDATE_ID),
            label: "Version 1.2.3 \u{2014} Update to 1.3.0 available".into()
        }
    );
    assert_eq!(with.entries[2].id, Some(OPEN_ID));
    let current = view(&status, Os::MacOs, Appearance::Light, Some(&update(None)));
    assert_eq!(current.entries[1].id, None);
    assert_eq!(current.entries[1].label, "Version 1.2.3");
}

#[test]
fn the_update_line_installs_the_update() {
    assert_eq!(action(UPDATE_ID), Some(Action::Update));
}

#[tokio::test(flavor = "multi_thread")]
async fn what_update_check_found_is_shown_with_the_status() {
    let f = fixture_offline().await;
    let status = status_of(&f.host).await;
    let port = FakeTray::default();
    let tray = TrayController::new(Box::new(port.clone()), Os::Linux, Appearance::Light);
    // Nothing to draw until a status arrives.
    show(&tray, Ok(update(Some("1.3.0"))));
    assert!(port.views.lock().unwrap().is_empty());
    tray.update_status(status.clone());
    show(&tray, Ok(update(Some("1.4.0"))));
    let views = port.views.lock().unwrap();
    assert_eq!(views.len(), 2);
    assert_eq!(
        views[0],
        view(
            &status,
            Os::Linux,
            Appearance::Light,
            Some(&update(Some("1.3.0")))
        )
    );
    assert_eq!(
        views[1],
        view(
            &status,
            Os::Linux,
            Appearance::Light,
            Some(&update(Some("1.4.0")))
        )
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_check_leaves_the_tray_as_it_was() {
    let f = fixture_offline().await;
    let port = FakeTray::default();
    let tray = TrayController::new(Box::new(port.clone()), Os::Linux, Appearance::Light);
    tray.update_status(status_of(&f.host).await);
    let failed = check(&f.host.control()).await;
    let RpcError { message, .. } = failed.clone().unwrap_err();
    assert!(!message.is_empty());
    show(&tray, failed);
    assert_eq!(port.views.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_tray_keeps_asking_update_check() {
    let f = fixture_offline().await;
    let port = FakeTray::default();
    let tray = Arc::new(TrayController::new(
        Box::new(port.clone()),
        Os::Linux,
        Appearance::Light,
    ));
    tray.update_status(status_of(&f.host).await);
    let task = tokio::spawn(poll(f.host.control(), tray));
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(!task.is_finished());
    task.abort();
}

#[tokio::test(flavor = "multi_thread")]
#[should_panic(expected = "the offered release can be installed")]
async fn installing_with_nothing_offered_is_reported() {
    let f = fixture_offline().await;
    apply(&f.host.control()).await;
}

#[test]
fn the_key_signs_what_the_updater_will_check() {
    let key = Key::new([5; 32]);
    let signature = key.sign(b"data");
    minisign::verify(&key.pubkey(), &signature, b"data").unwrap();
    assert!(minisign::verify(&key.pubkey(), &signature, b"other").is_err());
    let other = Key::new([6; 32]);
    assert!(minisign::verify(&other.pubkey(), &signature, b"data").is_err());
}

#[test]
fn a_request_that_arrives_in_pieces_is_still_answered() {
    let server = Server::start(&Key::new([5; 32]), &bundle());
    let port = server
        .endpoint()
        .split(':')
        .nth(2)
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET /bun").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    stream.write_all(b"dle HTTP/1.1\r\nHost: x\r\n").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    stream.write_all(b"\r\n").unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 200 OK"), "{reply}");
    assert!(reply.ends_with("the new Mouser"));
    server.stop();
}

#[test]
fn a_request_that_ends_early_is_answered_not_found() {
    let server = Server::start(&Key::new([5; 32]), &bundle());
    let port = server
        .endpoint()
        .split(':')
        .nth(2)
        .unwrap()
        .split('/')
        .next()
        .unwrap()
        .to_string();
    let mut stream = TcpStream::connect(format!("127.0.0.1:{port}")).unwrap();
    stream.write_all(b"GET").unwrap();
    stream.shutdown(std::net::Shutdown::Write).unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 404"), "{reply}");
    server.stop();
}

#[test]
fn the_server_gives_the_updater_a_manifest_and_the_bundle_then_stops() {
    let key = Key::new([5; 32]);
    let server = Server::start(&key, &bundle());
    let endpoint = server.endpoint();
    let (status, body) = get(&endpoint, "/latest.json");
    assert_eq!(status, "HTTP/1.1 200 OK");
    let manifest: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(manifest["version"], "1.3.0");
    let entry = &manifest["platforms"][TARGET];
    let url = entry["url"].as_str().unwrap();
    assert!(url.starts_with("http://127.0.0.1:") && url.ends_with("/bundle"));
    let (status, bytes) = get(&endpoint, "/bundle");
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(bytes, b"the new Mouser");
    minisign::verify(&key.pubkey(), entry["signature"].as_str().unwrap(), &bytes).unwrap();
    assert_eq!(get(&endpoint, "/elsewhere").0, "HTTP/1.1 404 Not Found");
    server.stop();
    assert!(
        TcpStream::connect(
            endpoint
                .strip_prefix("http://")
                .unwrap()
                .split('/')
                .next()
                .unwrap()
        )
        .is_err()
    );
}

#[derive(Default)]
struct Recording {
    seen: Mutex<Vec<(String, Vec<u8>)>>,
    fail: bool,
}

impl Applier for Recording {
    fn install(&self, endpoint: &str, target: &str, pubkey: &str) -> Result<(), String> {
        assert_eq!(target, TARGET);
        let (_, manifest) = get(endpoint, "/latest.json");
        let manifest: serde_json::Value = serde_json::from_slice(&manifest).unwrap();
        let (_, bytes) = get(endpoint, "/bundle");
        minisign::verify(
            pubkey,
            manifest["platforms"][TARGET]["signature"].as_str().unwrap(),
            &bytes,
        )
        .unwrap();
        self.seen
            .lock()
            .unwrap()
            .push((manifest["version"].as_str().unwrap().to_string(), bytes));
        if self.fail {
            Err("the updater said no".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn a_verified_bundle_reaches_the_updater_signed_and_the_server_is_gone_afterwards() {
    let applier = Arc::new(Recording::default());
    let install = installer(applier.clone());
    install(bundle()).unwrap();
    assert_eq!(
        *applier.seen.lock().unwrap(),
        vec![("1.3.0".to_string(), b"the new Mouser".to_vec())]
    );
}

#[test]
fn an_installer_failure_is_reported_to_the_engine() {
    let applier = Arc::new(Recording {
        fail: true,
        ..Default::default()
    });
    assert_eq!(
        installer(applier)(bundle()),
        Err("the updater said no".to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn update_check_reports_the_running_version_when_the_release_is_not_newer() {
    let old = Bundle {
        version: "1.0.0".into(),
        bytes: b"old".to_vec(),
    };
    let server = Server::start(&Key::new([5; 32]), &old);
    let f = fixture_releasing_at(&server.endpoint()).await;
    assert_eq!(check(&f.host.control()).await.unwrap(), update(None));
    server.stop();
}
