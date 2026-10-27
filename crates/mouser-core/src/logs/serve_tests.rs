//! A desk-mate asks for this computer's logs (wire-protocol-delta "Logs
//! stream"): one unidirectional stream answers.

use std::time::Duration;

use mouser_proto::control::{Desk, Method};
use mouser_proto::wire::{ComputerMode, ControlMessage, LogsCard, LogsPart, logs_stream_id};
use serde_json::json;

use crate::session::fault_tests::closed_with;
use crate::testing::*;

const REQUEST: [u8; 16] = [7; 16];

/// What came down the stream answering a request.
struct Answer {
    id: [u8; 16],
    part: LogsPart,
    files: Vec<Vec<u8>>,
    rest: Vec<u8>,
}

async fn ask(link: &mut crate::transport::Link) -> Answer {
    link.writer
        .send(&ControlMessage::LogsRequest { request: REQUEST })
        .await
        .unwrap();
    let mut stream = tokio::time::timeout(Duration::from_secs(10), link.conn.accept_uni())
        .await
        .expect("the stream arrives in time")
        .expect("the connection stays up");
    let mut id = [0u8; 16];
    stream.read_exact(&mut id).await.unwrap();
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).await.unwrap();
    let mut header = vec![0u8; u32::from_be_bytes(len) as usize];
    stream.read_exact(&mut header).await.unwrap();
    let part: LogsPart = postcard::from_bytes(&header).unwrap();
    let mut files = Vec::new();
    for (_, len) in &part.files {
        let mut bytes = vec![0u8; *len as usize];
        stream.read_exact(&mut bytes).await.unwrap();
        files.push(bytes);
    }
    let rest = stream.read_to_end(1 << 20).await.unwrap();
    Answer {
        id,
        part,
        files,
        rest,
    }
}

async fn member_of_a_desk(node: &TestNode) -> (Desk, crate::transport::Link) {
    let desk: Desk = call(node, Method::DeskCreate, json!({"name": "Home office"})).await;
    let link = member_rogue(node, &desk, 0x81).await;
    (desk, link)
}

#[tokio::test]
async fn a_desk_member_gets_the_card_the_config_and_every_log_file_in_order() {
    use mouser_platform::Platform;
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let (_desk, mut link) = member_of_a_desk(&node).await;
    // An older file, as after a rotation.
    let logs = node.dir.path().join("logs");
    std::fs::write(logs.join("mouser.log.1"), b"{\"older\":true}\n").unwrap();
    let answer = ask(&mut link).await;
    assert_eq!(answer.id, logs_stream_id(&REQUEST));
    let info = node.platform.info();
    assert_eq!(
        answer.part.card,
        LogsCard {
            id: node.engine.computer_id(),
            name: "Alpha".into(),
            os: crate::platform_os(info.os),
            os_version: info.version,
            version: "0.1.0".into(),
            mode: ComputerMode::Installed,
        }
    );
    // The settings file, as it is on disk.
    let config = std::fs::read_to_string(node.dir.path().join("config.toml")).unwrap();
    assert_eq!(answer.part.config_toml, config);
    assert!(answer.part.logging);
    let names: Vec<_> = answer.part.files.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["mouser.log", "mouser.log.1"]);
    // The bytes of each file follow, whole, and then the stream ends.
    let current = std::fs::read(logs.join("mouser.log")).unwrap();
    assert!(
        current.starts_with(&answer.files[0]),
        "a snapshot of the log"
    );
    assert!(answer.files[0].starts_with(b"{\"ts\""));
    assert_eq!(answer.files[1], b"{\"older\":true}\n");
    assert!(answer.rest.is_empty());
}

#[tokio::test]
async fn with_logging_off_the_answer_has_the_card_and_the_config_and_no_files() {
    let lab = Lab::new();
    let node = lab.start("Alpha").await;
    let (_desk, mut link) = member_of_a_desk(&node).await;
    let answer = ask(&mut link).await;
    assert!(!answer.part.logging);
    assert!(answer.part.files.is_empty());
    assert!(answer.files.is_empty());
    assert!(answer.part.config_toml.contains("[diagnostics]"));
    assert_eq!(answer.part.card.name, "Alpha");
}

#[tokio::test]
async fn a_computer_that_is_not_on_the_desk_gets_nothing() {
    let lab = Lab::new();
    let node = lab.start_logged("Alpha").await;
    let _desk: Desk = call(&node, Method::DeskCreate, json!({"name": "Home office"})).await;
    let rogue = Rogue::new(0x82);
    let (mut link, _) = rogue.dial(&node, None).await;
    link.writer
        .send(&ControlMessage::LogsRequest { request: REQUEST })
        .await
        .unwrap();
    // The link is closed as one that broke the protocol, and no stream came.
    assert_eq!(closed_with(&link.conn).await, b"protocol");
    assert!(link.conn.accept_uni().await.is_err());
}
