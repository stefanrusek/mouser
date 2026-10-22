use std::time::Duration;

use mouser_proto::control::{Method, UpdateResult};
use mouser_proto::ids::ComputerId;
use mouser_proto::release::ReleaseManifest;
use serde_json::json;

use super::Bundle;
use super::testing::{Server, Signer, computer, installer, publish};
use crate::testing::*;

fn key_of(node: &TestNode) -> String {
    node.engine.ctx.platform_key()
}

async fn available(node: &TestNode, version: &str) {
    eventually("the release to be offered", || async {
        node.engine.ctx.update_result().available.as_deref() == Some(version)
    })
    .await;
}

/// The node whose id is lowest is the one that goes to the internet.
fn lowest(nodes: &[TestNode]) -> usize {
    (0..nodes.len())
        .min_by_key(|i| nodes[*i].engine.computer_id())
        .unwrap()
}

#[tokio::test]
async fn only_the_fetcher_downloads_and_the_others_get_the_release_over_the_desk() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    let published = publish(&server, &signer, "0.2.0", &[]);
    let (install, installed) = installer();
    let mut nodes = Vec::new();
    for name in ["Alpha", "Beta", "Gamma"] {
        nodes.push(
            computer(
                &lab,
                name,
                "0.1.0",
                false,
                (&server, &signer),
                install.clone(),
            )
            .await,
        );
    }
    let nodes = desk_from(nodes).await;
    let f = lowest(&nodes);

    // A computer that is not elected never contacts the release location.
    for (i, node) in nodes.iter().enumerate().filter(|(i, _)| *i != f) {
        let result: UpdateResult = call(node, Method::UpdateCheck, json!({})).await;
        assert_eq!(result.available, None, "{i}");
    }
    assert!(server.requests.lock().unwrap().is_empty());

    let result: UpdateResult = call(&nodes[f], Method::UpdateCheck, json!({})).await;
    assert_eq!(
        result,
        UpdateResult {
            current: "0.1.0".into(),
            available: Some("0.2.0".into())
        }
    );
    let downloads = server.requests.lock().unwrap().clone();
    assert_eq!(
        downloads.len(),
        4,
        "latest.json and the three bundles once: {downloads:?}"
    );

    for (i, node) in nodes.iter().enumerate().filter(|(i, _)| *i != f) {
        available(node, "0.2.0").await;
        // The URLs it was offered lead to the fetcher, not the internet.
        let offered = node
            .engine
            .ctx
            .core
            .lock()
            .unwrap()
            .release
            .offered
            .clone()
            .unwrap();
        assert_eq!(offered.from, nodes[f].engine.computer_id());
        for (platform, asset) in &offered.manifest.platforms {
            assert_eq!(
                asset.url,
                format!("mouser-lan://{}/{platform}", nodes[f].engine.computer_id()),
                "{i}"
            );
        }
        let _: UpdateResult = call(node, Method::UpdateApply, json!({})).await;
    }
    {
        let installed = installed.lock().unwrap();
        assert_eq!(installed.len(), 2);
        for node in nodes
            .iter()
            .filter(|n| n.engine.computer_id() != nodes[f].engine.computer_id())
        {
            let expected = Bundle {
                version: "0.2.0".into(),
                bytes: published.bundles[&key_of(node)].clone(),
            };
            assert!(installed.contains(&expected));
        }
    }
    // Nobody but the fetcher downloaded anything.
    assert_eq!(server.requests.lock().unwrap().len(), 4);

    // The fetcher installs its own copy from what it stored.
    let _: UpdateResult = call(&nodes[f], Method::UpdateApply, json!({})).await;
    let installed = installed.lock().unwrap();
    assert_eq!(installed.len(), 3);
    assert!(installed.contains(&Bundle {
        version: "0.2.0".into(),
        bytes: published.bundles[&key_of(&nodes[f])].clone(),
    }));
}

#[tokio::test]
async fn a_computer_that_joins_later_is_offered_the_release_if_it_is_older() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    let published = publish(&server, &signer, "0.2.0", &[]);
    let (install, installed) = installer();
    let alpha = computer(
        &lab,
        "Alpha",
        "0.1.0",
        false,
        (&server, &signer),
        install.clone(),
    )
    .await;
    let beta = computer(
        &lab,
        "Beta",
        "0.1.0",
        false,
        (&server, &signer),
        install.clone(),
    )
    .await;
    let gamma = computer(
        &lab,
        "Gamma",
        "0.2.0",
        false,
        (&server, &signer),
        install.clone(),
    )
    .await;
    let _: mouser_proto::control::Desk =
        call(&alpha, Method::DeskCreate, json!({"name": "Home"})).await;
    let result: UpdateResult = call(&alpha, Method::UpdateCheck, json!({})).await;
    assert_eq!(result.available.as_deref(), Some("0.2.0"));
    admit(&alpha, &beta).await;
    admit(&alpha, &gamma).await;
    available(&beta, "0.2.0").await;
    let _: UpdateResult = call(&beta, Method::UpdateApply, json!({})).await;
    assert_eq!(
        installed.lock().unwrap().as_slice(),
        [Bundle {
            version: "0.2.0".into(),
            bytes: published.bundles[&key_of(&beta)].clone(),
        }]
    );
    // Gamma already runs 0.2.0: nothing to offer it.
    settle().await;
    assert_eq!(gamma.engine.ctx.update_result().available, None);
}

#[tokio::test]
async fn a_stored_release_is_offered_again_after_a_restart() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let (install, _) = installer();
    let alpha = computer(&lab, "Alpha", "0.1.0", false, (&server, &signer), install).await;
    let _: UpdateResult = call(&alpha, Method::UpdateCheck, json!({})).await;
    let platform = alpha.platform.clone();
    alpha.engine.shutdown().await;
    let alpha = lab.boot("Alpha", platform, alpha.dir).await;
    assert_eq!(
        alpha.engine.ctx.update_result().available.as_deref(),
        Some("0.2.0")
    );
}

fn manifest_for(node: &TestNode, version: &str, keys: &[String]) -> ReleaseManifest {
    ReleaseManifest {
        version: version.into(),
        notes: None,
        pub_date: None,
        min_protocol: None,
        platforms: keys
            .iter()
            .map(|k| {
                (
                    k.clone(),
                    mouser_proto::release::PlatformAsset {
                        url: format!("mouser-lan://{}/{k}", node.engine.computer_id()),
                        signature: "sig".into(),
                    },
                )
            })
            .collect(),
    }
}

#[tokio::test]
async fn offers_that_are_not_an_update_for_this_computer_are_ignored() {
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
    let (alpha, beta) = (&nodes[0], &nodes[1]);
    let ctx = alpha.engine.ctx.clone();
    let from = beta.engine.computer_id();
    let mine = vec![key_of(alpha)];
    let offer = |m: &ReleaseManifest| serde_json::to_string(m).unwrap();
    // Not newer, or without a bundle for this computer: ignored.
    for ignored in [
        manifest_for(beta, "0.1.0", &mine),
        manifest_for(beta, "0.0.9", &mine),
        manifest_for(beta, "0.5.0", &["other-platform".to_string()]),
    ] {
        ctx.on_release_available(from, &offer(&ignored)).unwrap();
        assert_eq!(ctx.update_result().available, None);
    }
    // A newer one is kept, and a later, older offer does not replace it.
    ctx.on_release_available(from, &offer(&manifest_for(beta, "0.3.0", &mine)))
        .unwrap();
    ctx.on_release_available(from, &offer(&manifest_for(beta, "0.2.0", &mine)))
        .unwrap();
    assert_eq!(ctx.update_result().available.as_deref(), Some("0.3.0"));
    ctx.on_release_available(from, &offer(&manifest_for(beta, "0.4.0", &mine)))
        .unwrap();
    assert_eq!(ctx.update_result().available.as_deref(), Some("0.4.0"));
    // Protocol breaches.
    assert!(
        ctx.on_release_available(from, "{")
            .unwrap_err()
            .contains("malformed manifest")
    );
    assert!(
        ctx.on_release_available(from, &offer(&manifest_for(beta, "soon", &mine)))
            .unwrap_err()
            .contains("not a version")
    );
    let stranger = ComputerId([9; 16]);
    assert!(
        ctx.on_release_available(stranger, &offer(&manifest_for(beta, "0.9.0", &mine)))
            .unwrap_err()
            .contains("not on the desk")
    );
    assert!(
        ctx.on_release_fetch(stranger, "0.9.0".into(), "x".into())
            .unwrap_err()
            .contains("not on the desk")
    );
}

#[tokio::test]
async fn a_release_nobody_holds_any_more_arrives_empty_and_is_refused() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    let (install, installed) = installer();
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
    let (alpha, beta) = (&nodes[0], &nodes[1]);
    let mut manifest = manifest_for(beta, "0.3.0", &[key_of(alpha)]);
    manifest
        .platforms
        .values_mut()
        .for_each(|a| a.signature = signer.sign(b"the bundle", true));
    alpha
        .engine
        .ctx
        .on_release_available(
            beta.engine.computer_id(),
            &serde_json::to_string(&manifest).unwrap(),
        )
        .unwrap();
    let err = call_err(alpha, Method::UpdateApply, json!({})).await;
    assert!(
        err.message.contains("the bundle was not installed"),
        "{}",
        err.message
    );
    assert!(installed.lock().unwrap().is_empty());
    // The failure is not sticky: a computer may try again.
    assert!(!alpha.engine.ctx.core.lock().unwrap().release.applying);
}

#[tokio::test]
async fn automatic_updates_install_without_being_asked() {
    let (lab, server, signer) = (Lab::new(), Server::start().await, Signer::new(1));
    publish(&server, &signer, "0.2.0", &[]);
    let (install, installed) = installer();
    let nodes = vec![
        computer(
            &lab,
            "Alpha",
            "0.1.0",
            true,
            (&server, &signer),
            install.clone(),
        )
        .await,
        computer(
            &lab,
            "Beta",
            "0.1.0",
            true,
            (&server, &signer),
            install.clone(),
        )
        .await,
        computer(&lab, "Gamma", "0.1.0", true, (&server, &signer), install).await,
    ];
    let nodes = desk_from(nodes).await;
    let f = lowest(&nodes);
    let _: UpdateResult = call(&nodes[f], Method::UpdateCheck, json!({})).await;
    eventually("every computer to install the release", || async {
        installed.lock().unwrap().len() == 3
    })
    .await;
    // Same-version peers never download it either.
    assert_eq!(server.requests.lock().unwrap().len(), 4);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(installed.lock().unwrap().len(), 3);
}
