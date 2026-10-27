//! Privacy contract test (SC-004, FR-003): what is typed and copied on the
//! computers of a desk with logging on never reaches a log. The marker is
//! typed as key events, copied as clipboard text and copied as a file name,
//! on both computers, and then a bundle is collected: no log file and no
//! entry of the bundle holds it.

mod steps;
mod support;

use std::time::{Duration, Instant};

use mouser_core::crossing::panic::usage_for_name;
use mouser_platform::{ClipRep, LocalInput};
use mouser_proto::control::{LogsCollectResult, Method};
use mouser_testkit::{Injected, VirtualPlatform};
use serde_json::json;
use support::desk::{Lab, Setup};
use support::logs::Zip;

const MARKER: &str = "MOUSER-MARKER-7f3a";

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The marker, key by key: the letters and digits of it.
fn type_marker(platform: &VirtualPlatform) {
    for c in MARKER
        .to_uppercase()
        .chars()
        .filter(char::is_ascii_alphanumeric)
    {
        let usage = usage_for_name(&c.to_string()).unwrap();
        for down in [true, false] {
            platform.send_input(LocalInput::Key { usage, down });
        }
    }
}

fn keys_injected(platform: &VirtualPlatform) -> usize {
    platform
        .injected()
        .iter()
        .filter(|i| matches!(i, Injected::Key { down: true, .. }))
        .count()
}

#[tokio::test]
async fn what_is_typed_and_copied_is_in_no_log_and_no_bundle() {
    let mut lab = Lab::new();
    let setup = || {
        let mut setup = Setup {
            mouser_version: "0.3.0".into(),
            ..Setup::default()
        };
        setup.config.diagnostics.logging = true;
        setup
    };
    lab.form_desk(
        "Home office",
        "Alpha",
        &["Beta"],
        vec![("Alpha", setup()), ("Beta", setup())],
    )
    .await;
    let (alpha, beta) = (
        lab.node("Alpha").platform.clone(),
        lab.node("Beta").platform.clone(),
    );

    // Typed on Alpha's keyboard while Alpha has the pointer, and while
    // Alpha's keyboard drives Beta.
    type_marker(&alpha);
    alpha.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (1919.0, 540.0),
    });
    until("Alpha to drive Beta", || alpha.is_grabbed()).await;
    until("Beta to have the pointer", || {
        beta.injected()
            .iter()
            .any(|i| matches!(i, Injected::MotionAbs { .. }))
    })
    .await;
    type_marker(&alpha);
    until("the typing to reach Beta", || keys_injected(&beta) > 0).await;
    alpha.send_input(LocalInput::Motion {
        dx: -1.0,
        dy: 0.0,
        abs: (960.0, 540.0),
    });
    until("the pointer to come back", || !alpha.pointer_hidden()).await;
    // And on Beta's keyboard, driving Alpha.
    type_marker(&beta);
    beta.send_input(LocalInput::Motion {
        dx: -1.0,
        dy: 0.0,
        abs: (0.0, 540.0),
    });
    until("Beta to drive Alpha", || beta.is_grabbed()).await;
    type_marker(&beta);
    until("the typing to reach Alpha", || keys_injected(&alpha) > 0).await;
    beta.send_input(LocalInput::Motion {
        dx: 1.0,
        dy: 0.0,
        abs: (960.0, 540.0),
    });
    until("the pointer to come back to Beta", || {
        !beta.pointer_hidden()
    })
    .await;

    // Copied: as text, and as a file whose name holds it, on both.
    let folder = tempfile::tempdir().unwrap();
    for (platform, owner) in [(&alpha, "alpha"), (&beta, "beta")] {
        let file = folder.path().join(format!("{owner}-{MARKER}.txt"));
        std::fs::write(&file, format!("contents {MARKER}")).unwrap();
        platform.set_local_clipboard(vec![
            ClipRep::Text(format!("copied on {owner}: {MARKER}")),
            ClipRep::Files(vec![file]),
        ]);
        let other = if owner == "alpha" { &beta } else { &alpha };
        until("the copy to reach the other computer", || {
            !other.clipboard_contents().is_empty()
                && other.clipboard_contents()
                    != vec![ClipRep::Text(format!(
                        "copied on {}: {MARKER}",
                        if owner == "alpha" { "beta" } else { "alpha" }
                    ))]
        })
        .await;
    }

    // The desk's logs, in one bundle.
    let out = tempfile::tempdir().unwrap();
    let result: LogsCollectResult = lab
        .node("Alpha")
        .call(
            Method::LogsCollect,
            json!({"dir": out.path().to_str().unwrap()}),
        )
        .await;
    assert!(result.computers.iter().all(|c| c.included));

    let mut found = 0;
    for name in ["Alpha", "Beta"] {
        let logs = lab.node(name).dir.path().join("logs");
        for file in std::fs::read_dir(&logs).unwrap() {
            let text = std::fs::read(file.unwrap().path()).unwrap();
            found += 1;
            assert!(
                !String::from_utf8_lossy(&text).contains(MARKER),
                "{name}'s log holds the marker"
            );
        }
    }
    assert!(found >= 2, "both computers have a log file");
    let mut zip = Zip::open(std::path::Path::new(&result.path));
    for entry in zip.names() {
        let text = String::from_utf8_lossy(&zip.bytes(&entry)).into_owned();
        assert!(!entry.contains(MARKER));
        assert!(!text.contains(MARKER), "{entry} holds the marker");
        // Not in pieces either: the typed form of it, or its file name.
        assert!(!text.contains("MOUSERMARKER"), "{entry}");
    }
    lab.shutdown().await;
}
