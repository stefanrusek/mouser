use std::path::Path;

use super::Version;
use super::fetch::check;
use super::http::Http;
use super::serve::Held;
use super::testing::{Server, Signer, platform_keys, publish};

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

async fn check_at(
    server: &Server,
    path: &str,
    signer: &Signer,
    folder: &Path,
) -> Result<Option<Held>, String> {
    check(
        &Http::system(),
        &server.url(path),
        &signer.pubkey(),
        &v("0.1.0"),
        folder,
    )
    .await
}

#[test]
fn versions_order_as_semantic_versions() {
    assert!(v("0.2.0") > v("0.1.9"));
    assert!(v("v1.0.0") > v("0.99.99"));
    assert!(v("1.0.0") > v("1.0.0-beta.1"));
    assert!(v("1.0.0-beta.2") > v("1.0.0-beta.1"));
    assert_eq!(v("1.0.0-rc"), v("v1.0.0-rc"));
    assert!(v("1.0.0-beta.1") < v("1.0.0"));
    assert!(v("1.0.0").partial_cmp(&v("1.0.0")).unwrap().is_eq());
    for bad in ["1.0", "1.0.x", "one", "1.2.3.4"] {
        assert_eq!(
            Version::parse(bad).unwrap_err(),
            format!("{bad:?} is not a version")
        );
    }
}

#[tokio::test]
async fn a_newer_release_is_downloaded_in_full_and_verified_per_platform() {
    let (server, signer, dir) = (
        Server::start().await,
        Signer::new(1),
        tempfile::tempdir().unwrap(),
    );
    let published = publish(&server, &signer, "0.2.0", &[]);
    let held = check_at(&server, "/latest.json", &signer, dir.path())
        .await
        .unwrap()
        .expect("0.2.0 is newer than 0.1.0");
    assert_eq!(held.manifest.version, "0.2.0");
    assert_eq!(platform_keys().len(), held.manifest.platforms.len());
    for (key, bytes) in &published.bundles {
        assert_eq!(held.bundle(key).as_ref(), Some(bytes), "{key}");
    }
    // The stored release is found again after a restart.
    assert_eq!(Held::load(dir.path()).unwrap().manifest, held.manifest);
}

#[tokio::test]
async fn a_release_that_is_not_newer_downloads_nothing() {
    let (server, signer, dir) = (
        Server::start().await,
        Signer::new(1),
        tempfile::tempdir().unwrap(),
    );
    publish(&server, &signer, "0.1.0", &[]);
    assert!(
        check_at(&server, "/latest.json", &signer, dir.path())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(server.requests.lock().unwrap().as_slice(), ["/latest.json"]);
}

#[tokio::test]
async fn an_unverified_bundle_is_not_offered() {
    let (server, signer, dir) = (
        Server::start().await,
        Signer::new(1),
        tempfile::tempdir().unwrap(),
    );
    publish(&server, &signer, "0.2.0", &["windows"]);
    let held = check_at(&server, "/latest.json", &signer, dir.path())
        .await
        .unwrap()
        .unwrap();
    let keys: Vec<_> = held.manifest.platforms.keys().cloned().collect();
    assert!(!keys.iter().any(|k| k.starts_with("windows")), "{keys:?}");
    assert_eq!(keys.len(), 2);
    assert!(
        held.bundle(&format!("windows-{}", std::env::consts::ARCH))
            .is_none()
    );
    // Signed by another key: nothing verifies, so there is no release.
    let err = check_at(&server, "/latest.json", &Signer::new(2), dir.path())
        .await
        .unwrap_err();
    assert_eq!(err, "no bundle of 0.2.0 could be verified");
}

#[tokio::test]
async fn a_bundle_shared_by_platforms_is_downloaded_once() {
    let (server, signer, dir) = (
        Server::start().await,
        Signer::new(1),
        tempfile::tempdir().unwrap(),
    );
    let sig = signer.sign(b"universal", true);
    server.serve("/mac.tar.gz", b"universal");
    let entry = serde_json::json!({"url": server.url("/mac.tar.gz"), "signature": sig});
    let manifest = serde_json::json!({"version": "v0.2.0", "platforms": {
        "darwin-aarch64": entry, "darwin-x86_64": entry}});
    server.serve("/latest.json", manifest.to_string().as_bytes());
    let held = check_at(&server, "/latest.json", &signer, dir.path())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(held.bundle("darwin-x86_64").unwrap(), b"universal");
    assert_eq!(held.bundle("darwin-aarch64").unwrap(), b"universal");
    assert_eq!(
        server
            .requests
            .lock()
            .unwrap()
            .iter()
            .filter(|r| *r == "/mac.tar.gz")
            .count(),
        1
    );
}

#[tokio::test]
async fn the_release_url_may_be_a_github_release_or_a_list_of_releases() {
    let (server, signer, dir) = (
        Server::start().await,
        Signer::new(1),
        tempfile::tempdir().unwrap(),
    );
    publish(&server, &signer, "0.2.0", &[]);
    let asset = |name: &str| serde_json::json!({"name": name, "browser_download_url": server.url("/latest.json")});
    // `releases/latest`: one release object.
    server.serve(
        "/latest",
        serde_json::json!({"assets": [asset("Mouser.dmg"), asset("latest.json")]})
            .to_string()
            .as_bytes(),
    );
    let held = check_at(&server, "/latest", &signer, dir.path())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(held.manifest.version, "0.2.0");
    // `releases`: newest first, including pre-releases; drafts and releases
    // without a manifest are passed over.
    server.serve(
        "/list",
        serde_json::json!([
            {"draft": true, "assets": [asset("latest.json")]},
            {"draft": false, "assets": [asset("Mouser.dmg")]},
            {"draft": false, "prerelease": true, "assets": [asset("latest.json")]},
        ])
        .to_string()
        .as_bytes(),
    );
    assert!(
        check_at(&server, "/list", &signer, dir.path())
            .await
            .unwrap()
            .is_some()
    );
    server.serve("/none", b"[]");
    assert_eq!(
        check_at(&server, "/none", &signer, dir.path())
            .await
            .unwrap_err(),
        format!("{} offers no latest.json", server.url("/none"))
    );
    server.serve(
        "/noasset",
        serde_json::json!({"assets": [{"name": "latest.json"}]})
            .to_string()
            .as_bytes(),
    );
    assert!(
        check_at(&server, "/noasset", &signer, dir.path())
            .await
            .unwrap_err()
            .contains("offers no latest.json")
    );
}

#[tokio::test]
async fn unusable_answers_are_errors() {
    let (server, signer, dir) = (
        Server::start().await,
        Signer::new(1),
        tempfile::tempdir().unwrap(),
    );
    server.serve("/text", b"not json");
    assert!(
        check_at(&server, "/text", &signer, dir.path())
            .await
            .unwrap_err()
            .contains("did not answer with JSON")
    );
    server.serve("/shape", br#"{"version": 1, "platforms": {}}"#);
    assert!(
        check_at(&server, "/shape", &signer, dir.path())
            .await
            .unwrap_err()
            .contains("not a release manifest")
    );
    server.serve("/version", br#"{"version": "soon", "platforms": {}}"#);
    assert_eq!(
        check_at(&server, "/version", &signer, dir.path())
            .await
            .unwrap_err(),
        "\"soon\" is not a version"
    );
    server.serve("/gone", br#"{"version": "0.3.0", "platforms": {"linux-x86_64": {"url": "http://127.0.0.1:1/x", "signature": "s"}}}"#);
    assert!(
        check_at(&server, "/gone", &signer, dir.path())
            .await
            .unwrap_err()
            .contains("cannot reach")
    );
}

#[test]
fn stored_releases_load_the_newest_and_ignore_junk() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Held::load(&dir.path().join("none")).is_none());
    let manifest = |version: &str| mouser_proto::release::ReleaseManifest {
        version: version.into(),
        notes: None,
        pub_date: None,
        min_protocol: None,
        platforms: Default::default(),
    };
    for (name, version) in [("a", "0.2.0"), ("b", "0.10.0"), ("c", "0.3.0")] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
        std::fs::write(
            dir.path().join(name).join("manifest.json"),
            serde_json::to_string(&manifest(version)).unwrap(),
        )
        .unwrap();
    }
    std::fs::create_dir(dir.path().join("empty")).unwrap();
    std::fs::create_dir(dir.path().join("broken")).unwrap();
    std::fs::write(dir.path().join("broken").join("manifest.json"), "{").unwrap();
    std::fs::create_dir(dir.path().join("odd")).unwrap();
    std::fs::write(
        dir.path().join("odd").join("manifest.json"),
        serde_json::to_string(&manifest("later")).unwrap(),
    )
    .unwrap();
    assert_eq!(Held::load(dir.path()).unwrap().manifest.version, "0.10.0");
}

#[test]
fn platform_keys_name_the_operating_system_and_architecture() {
    use mouser_platform::Os;
    assert_eq!(super::platform_key(Os::MacOs, "aarch64"), "darwin-aarch64");
    assert_eq!(super::platform_key(Os::Windows, "x86_64"), "windows-x86_64");
    assert_eq!(super::platform_key(Os::Linux, "x86_64"), "linux-x86_64");
}
