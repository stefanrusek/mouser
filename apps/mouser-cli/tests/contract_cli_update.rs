//! Contract tests for `mouser update [check|apply]` (Story 10 sc. 4): they call
//! `update.check` / `update.apply` on the running Mouser, against a local
//! release host.

use std::sync::{Arc, Mutex};

use crate::support::Lab;
use crate::support::release::{Host, Signer};
use mouser_core::release::{Bundle, Installer};
use mouser_testkit::VirtualPlatform;

fn installer() -> (Installer, Arc<Mutex<Vec<Bundle>>>) {
    let installed: Arc<Mutex<Vec<Bundle>>> = Arc::default();
    let seen = installed.clone();
    let installer: Installer = Arc::new(move |bundle| {
        seen.lock().unwrap().push(bundle);
        Ok(())
    });
    (installer, installed)
}

async fn released(
    lab: &Lab,
    host: &Host,
    signer: &Signer,
    installer: Installer,
) -> crate::support::Node {
    let url = host.release_url();
    let key = signer.pubkey();
    lab.add_tuned(
        "Alpha",
        VirtualPlatform::new(),
        |c| {
            c.updates.automatic = false;
            c.updates.release_url = url;
        },
        |p| {
            p.release_pubkey = key;
            p.installer = Some(installer);
        },
    )
    .await
}

#[tokio::test(flavor = "multi_thread")]
async fn check_prints_current_and_available_versions() {
    let (signer, host) = (Signer::new(), Host::start().await);
    let (installer, installed) = installer();
    let lab = Lab::new();
    let alpha = released(&lab, &host, &signer, installer).await;

    host.publish(&signer, "0.1.0");
    let none = alpha.mouser(&["update", "check"]).await;
    assert_eq!(
        (none.code, none.stdout.as_str()),
        (0, "Current version: 0.1.0\nAvailable version: none\n")
    );

    host.publish(&signer, "0.2.0");
    let found = alpha.mouser(&["update"]).await;
    assert_eq!(
        (found.code, found.stdout.as_str()),
        (0, "Current version: 0.1.0\nAvailable version: 0.2.0\n")
    );
    assert_eq!(
        alpha.mouser(&["--json", "update", "check"]).await.json(),
        serde_json::json!({"current": "0.1.0", "available": "0.2.0"})
    );
    assert!(installed.lock().unwrap().is_empty());
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_installs_the_offered_release() {
    let (signer, host) = (Signer::new(), Host::start().await);
    let (installer, installed) = installer();
    let lab = Lab::new();
    let alpha = released(&lab, &host, &signer, installer).await;
    let bundles = host.publish(&signer, "0.2.0");
    assert_eq!(alpha.mouser(&["update", "check"]).await.code, 0);

    let applied = alpha.mouser(&["update", "apply"]).await;
    assert_eq!(
        (applied.code, applied.stdout.as_str()),
        (0, "Updating from 0.1.0 to 0.2.0\n")
    );
    {
        let installed = installed.lock().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].version, "0.2.0");
        assert!(bundles.values().any(|b| *b == installed[0].bytes));
    }

    let json = alpha.mouser(&["--json", "update", "apply"]).await.json();
    assert_eq!(json["available"], "0.2.0");
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_without_an_update_says_so_and_exits_1() {
    let (signer, host) = (Signer::new(), Host::start().await);
    let (installer, installed) = installer();
    let lab = Lab::new();
    let alpha = released(&lab, &host, &signer, installer).await;
    let out = alpha.mouser(&["update", "apply"]).await;
    assert_eq!((out.code, out.stdout.as_str()), (1, ""));
    assert_eq!(out.stderr.trim(), "no update is available");
    assert!(installed.lock().unwrap().is_empty());
    lab.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn update_exits_2_when_mouser_is_not_running() {
    let lab = Lab::new();
    let nobody = lab.offline("Nobody");
    for args in [&["update"][..], &["update", "check"], &["update", "apply"]] {
        let out = nobody.mouser(args).await;
        assert_eq!(out.code, 2, "{args:?}");
        assert_eq!(out.stderr.trim(), "Mouser is not running");
    }
    let help = nobody.mouser(&["--help"]).await;
    assert!(help.stdout.contains("update"));
    lab.shutdown().await;
}
