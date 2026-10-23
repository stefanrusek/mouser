use std::sync::Arc;
use std::time::Duration;

use mouser_proto::control::{Method, UpdateResult};
use mouser_proto::ids::ComputerId;
use serde_json::json;

use crate::release::testing::{Server, Signer, computer, installer, publish};
use crate::release::{Bundle, CheckSchedule, Offered};
use crate::testing::*;

#[tokio::test]
async fn check_reports_the_running_and_the_available_version() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    let (install, _) = installer();
    let node = computer(&lab, "Alpha", "0.1.0", false, (&server, &signer), install).await;
    // Nothing published yet.
    let err = call_err(&node, Method::UpdateCheck, json!({})).await;
    assert_eq!(err.code, -32000);
    assert!(err.message.contains("answered 404"), "{}", err.message);
    publish(&server, &signer, "0.1.0", &[]);
    let result: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    assert_eq!(
        result,
        UpdateResult {
            current: "0.1.0".into(),
            available: None
        }
    );
    publish(&server, &signer, "0.2.0", &[]);
    let result: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    assert_eq!(
        result,
        UpdateResult {
            current: "0.1.0".into(),
            available: Some("0.2.0".into())
        }
    );
    // A release already held is not downloaded again.
    let before = server.requests.lock().unwrap().len();
    let _: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    assert_eq!(
        server.requests.lock().unwrap().len(),
        before + 1,
        "only latest.json"
    );
}

#[tokio::test]
async fn apply_hands_the_verified_bundle_to_the_installer() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    let published = publish(&server, &signer, "0.2.0", &[]);
    let (install, installed) = installer();
    let node = computer(&lab, "Alpha", "0.1.0", false, (&server, &signer), install).await;
    let err = call_err(&node, Method::UpdateApply, json!({})).await;
    assert_eq!(err.message, "no update is available");
    let _: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    let result: UpdateResult = call(&node, Method::UpdateApply, json!({})).await;
    assert_eq!(result.available.as_deref(), Some("0.2.0"));
    assert_eq!(
        installed.lock().unwrap().as_slice(),
        [Bundle {
            version: "0.2.0".into(),
            bytes: published.bundles[&node.engine.ctx.platform_key()].clone(),
        }]
    );
}

#[tokio::test]
async fn apply_refuses_what_it_cannot_do() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let (install, _) = installer();
    let node = computer(&lab, "Alpha", "0.1.0", false, (&server, &signer), install).await;
    let _: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    let ctx = node.engine.ctx.clone();
    let offered = ctx.core.lock().unwrap().release.offered.clone().unwrap();
    let apply_error = || async {
        call_err(&node, Method::UpdateApply, json!({}))
            .await
            .message
    };

    ctx.core.lock().unwrap().release.applying = true;
    assert_eq!(apply_error().await, "an update is already being installed");
    ctx.core.lock().unwrap().release.applying = false;

    // The release is offered by a computer that is not connected.
    ctx.core.lock().unwrap().release.offered = Some(Offered {
        from: ComputerId([7; 16]),
        manifest: offered.manifest.clone(),
    });
    assert_eq!(
        apply_error().await,
        "the computer with the update is not connected"
    );

    // A manifest without this computer's bundle.
    let mut other = offered.clone();
    other.manifest.platforms.clear();
    ctx.core.lock().unwrap().release.offered = Some(other);
    assert_eq!(apply_error().await, "no bundle for this computer");

    // The stored release or its bundle is gone.
    ctx.core.lock().unwrap().release.offered = Some(offered.clone());
    let held = ctx.core.lock().unwrap().release.held.take().unwrap();
    assert_eq!(apply_error().await, "the release is no longer stored");
    let mut empty = held.clone();
    empty.manifest.platforms.clear();
    ctx.core.lock().unwrap().release.held = Some(empty);
    assert_eq!(apply_error().await, "the bundle is no longer stored");
    ctx.core.lock().unwrap().release.held = Some(held);

    // A bundle that does not match its signature is never installed.
    let key = ctx.platform_key();
    let mut forged = offered.clone();
    forged.manifest.platforms.get_mut(&key).unwrap().signature =
        Signer::new(1).sign(b"other", true);
    ctx.core.lock().unwrap().release.offered = Some(forged);
    assert!(apply_error().await.contains("the bundle was not installed"));
    ctx.core.lock().unwrap().release.offered = Some(offered);
}

#[tokio::test]
async fn an_app_without_an_installer_cannot_apply() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let node = lab
        .start_released(
            "Alpha",
            |c| c.updates.release_url = server.url("/latest.json"),
            |p| {
                p.release_pubkey = signer.pubkey();
            },
        )
        .await;
    let _: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    let err = call_err(&node, Method::UpdateApply, json!({})).await;
    assert_eq!(err.message, "this copy of Mouser cannot install updates");
}

#[tokio::test]
async fn a_failing_installer_fails_the_apply() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let node = lab
        .start_released(
            "Alpha",
            |c| c.updates.release_url = server.url("/latest.json"),
            |p| {
                p.release_pubkey = signer.pubkey();
                p.installer = Some(Arc::new(|_| Err("disk full".into())));
            },
        )
        .await;
    let _: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    let err = call_err(&node, Method::UpdateApply, json!({})).await;
    assert_eq!(err.message, "disk full");
}

#[tokio::test]
async fn waiting_for_a_bundle_ends_when_the_computer_goes_away() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    let (install, _) = installer();
    let nodes = vec![
        computer(
            &lab,
            "Alpha",
            "0.1.0",
            false,
            (&server, &signer),
            install.clone(),
        )
        .await,
        computer(&lab, "Beta", "0.1.0", false, (&server, &signer), install).await,
    ];
    let nodes = desk_from(nodes).await;
    let ctx = nodes[0].engine.ctx.clone();
    let beta = nodes[1].engine.computer_id();
    let (tx, rx) = tokio::sync::oneshot::channel();
    ctx.release_waiters.lock().unwrap().push((
        crate::release::serve::stream_id("0.2.0", "x"),
        beta,
        tx,
    ));
    ctx.drop_release_waiters(beta);
    assert!(rx.await.is_err());
    assert!(ctx.release_waiters.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_schedule_checks_by_itself_when_automatic() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let (install, installed) = installer();
    let schedule = CheckSchedule {
        first: Duration::from_millis(50),
        every: Duration::from_millis(50),
    };
    let node = lab
        .start_released(
            "Alpha",
            |c| c.updates.release_url = server.url("/latest.json"),
            |p| {
                p.release_pubkey = signer.pubkey();
                p.installer = Some(install);
                p.check_schedule = Some(schedule);
            },
        )
        .await;
    eventually("the release to be installed", || async {
        !installed.lock().unwrap().is_empty()
    })
    .await;
    assert_eq!(
        node.engine.ctx.update_result().available.as_deref(),
        Some("0.2.0")
    );
}

#[tokio::test]
async fn the_schedule_does_nothing_unless_automatic_and_logs_failures() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let schedule = CheckSchedule {
        first: Duration::from_millis(20),
        every: Duration::from_millis(20),
    };
    let off = lab
        .start_released(
            "Alpha",
            |c| {
                c.updates.automatic = false;
                c.updates.release_url = server.url("/latest.json");
            },
            |p| p.check_schedule = Some(schedule),
        )
        .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(server.requests.lock().unwrap().is_empty());
    off.engine.shutdown().await;

    let broken = lab
        .start_released(
            "Beta",
            |c| {
                c.diagnostics.logging = true;
                c.updates.release_url = server.url("/missing.json");
            },
            |p| p.check_schedule = Some(schedule),
        )
        .await;
    let log = broken.dir.path().join("logs").join("mouser.log");
    eventually("the failure to be logged", || async {
        std::fs::read_to_string(&log).is_ok_and(|text| text.contains("update.check_failed"))
    })
    .await;
}

#[tokio::test]
async fn a_failed_automatic_update_is_logged() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let node = lab
        .start_released(
            "Alpha",
            |c| {
                c.diagnostics.logging = true;
                c.updates.release_url = server.url("/latest.json");
            },
            |p| p.release_pubkey = signer.pubkey(),
        )
        .await;
    // No installer: the automatic update has nothing to install with.
    let _: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    let log = node.dir.path().join("logs").join("mouser.log");
    eventually("the failure to be logged", || async {
        std::fs::read_to_string(&log).is_ok_and(|text| text.contains("update.failed"))
    })
    .await;
}

/// The app's installer downloads the bundle over the engine's own runtime
/// from a thread of its own and waits for it, then restarts. A lone fetcher
/// (no desk-mates online) updating by itself must leave the runtime free to
/// do that download, or the engine hangs with it (v0.2.2 on a desk of one).
#[tokio::test]
async fn a_lone_fetcher_installs_with_an_installer_that_needs_the_runtime() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let (runtime, port) = (tokio::runtime::Handle::current(), server.port);
    let installed: Arc<std::sync::Mutex<Vec<String>>> = Arc::default();
    let sink = installed.clone();
    let install: crate::release::Installer = Arc::new(move |bundle| {
        let (runtime, (tx, rx)) = (runtime.clone(), std::sync::mpsc::channel());
        std::thread::spawn(move || {
            let reply = runtime.block_on(async move {
                let mut stream = tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .unwrap();
                stream
                    .write_all(b"GET /latest.json HTTP/1.1\r\n\r\n")
                    .await
                    .unwrap();
                let mut reply = String::new();
                stream.read_to_string(&mut reply).await.unwrap();
                reply
            });
            let _ = tx.send(reply);
        });
        let reply = rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the installer can use the network while it installs");
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        sink.lock().unwrap().push(bundle.version);
        Ok(())
    });
    let schedule = CheckSchedule {
        first: Duration::from_millis(20),
        every: Duration::from_secs(3600),
    };
    let node = lab
        .start_released(
            "Alpha",
            |c| c.updates.release_url = server.url("/latest.json"),
            |p| {
                p.release_pubkey = signer.pubkey();
                p.installer = Some(install);
                p.check_schedule = Some(schedule);
            },
        )
        .await;
    eventually("the release to be installed", || async {
        !installed.lock().unwrap().is_empty()
    })
    .await;
    assert_eq!(installed.lock().unwrap().as_slice(), ["0.2.0"]);
    // The engine still answers while and after it installs.
    let result: UpdateResult = call(&node, Method::UpdateCheck, json!({})).await;
    assert_eq!(result.available.as_deref(), Some("0.2.0"));
}
