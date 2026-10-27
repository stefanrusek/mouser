//! `logs.collect_stream` (owner decision OD-7): the same bundle, in memory,
//! sent to the caller only.

use base64::Engine as _;
use mouser_proto::control::{LogsCollectResult, LogsCollectStreamResult, Method, Topic};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::ControlClient;
use crate::testing::*;

/// Text that hardly compresses, so that the bundle is longer than a chunk.
fn noise(len: usize) -> String {
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    (0..len)
        .map(|_| {
            x = x
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            char::from(b'a' + ((x >> 33) % 26) as u8)
        })
        .collect()
}

fn names(zip: &[u8]) -> Vec<String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().unwrap().to_string())
        .collect()
}

#[tokio::test]
async fn the_bundle_arrives_in_chunks_to_the_caller_alone_and_matches_its_size_and_hash() {
    let lab = Lab::new();
    let mut nodes = Vec::new();
    for name in ["Alpha", "Beta"] {
        nodes.push(lab.start_version(name, "0.3.0", true).await);
    }
    let nodes = desk_from(nodes).await;
    let a = &nodes[0];
    // Enough log to fill more than one chunk.
    for n in 0..2 {
        let _: mouser_proto::control::Empty = call(
            a,
            Method::ConfigSet,
            json!({"key": "updates.release_url", "value": format!("http://{n}{}", noise(40_000))}),
        )
        .await;
    }
    let mut caller = ControlClient::connect(&a.control_path).await.unwrap();
    let mut bystander = ControlClient::connect(&a.control_path).await.unwrap();
    bystander
        .subscribe(vec![Topic::Config, Topic::Status, Topic::Problem])
        .await
        .unwrap();

    let result: LogsCollectStreamResult = caller
        .call_typed(Method::LogsCollectStream, &json!({}))
        .await
        .unwrap();
    let chunks = caller.take_chunks();
    assert!(chunks.len() >= 2, "{} chunks", chunks.len());
    let mut zip = Vec::new();
    for (seq, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.seq as usize, seq, "from 0, in order");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&chunk.data)
            .unwrap();
        assert!(!bytes.is_empty() && bytes.len() <= 65536, "{}", bytes.len());
        zip.extend(bytes);
    }
    assert_eq!(result.size, zip.len() as u64);
    let digest = Sha256::digest(&zip)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(result.sha256, digest);
    assert!(
        result.name.starts_with("mouser-logs-Home-office-") && result.name.ends_with("Z.zip"),
        "{}",
        result.name
    );
    assert_eq!(
        result
            .computers
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        ["Alpha", "Beta"]
    );
    assert!(result.computers.iter().all(|c| c.included));

    // The same bundle as `logs.collect` writes.
    let out = tempfile::tempdir().unwrap();
    let written: LogsCollectResult = call(
        a,
        Method::LogsCollect,
        json!({"dir": out.path().to_str().unwrap()}),
    )
    .await;
    let file = std::fs::read(&written.path).unwrap();
    assert_eq!(names(&zip), names(&file));

    // Nobody else on the control API saw a chunk, and nothing was written.
    let _ = bystander.call(Method::Status, None).await.unwrap();
    assert!(bystander.take_chunks().is_empty());
    assert!(!a.dir.path().join("bundles").exists());
    let zips = std::fs::read_dir(a.dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "zip"))
        .count();
    assert_eq!(zips, 0);
}

#[tokio::test]
async fn the_in_process_handle_has_no_connection_to_send_chunks_to() {
    let lab = Lab::new();
    let node = lab.start_version("Alpha", "0.3.0", true).await;
    let result: LogsCollectStreamResult = call(&node, Method::LogsCollectStream, json!({})).await;
    // The caller still gets the result: it describes the bundle.
    assert!(result.size > 0);
    assert_eq!(result.computers.len(), 1);
}
