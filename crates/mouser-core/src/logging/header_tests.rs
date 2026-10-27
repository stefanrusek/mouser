//! Every log file starts with `log.header` (Story 2 scenario 1), also the
//! one a rotation opens.

use mouser_proto::ids::{ComputerId, DeskId};
use mouser_proto::model::Os;
use mouser_proto::wire::ComputerMode;
use serde_json::Value;

use super::events::{self, LinkLossReason};
use super::events_tests::validator;
use super::{Context, Log};

const ME: ComputerId = ComputerId([7; 16]);
const DESK: DeskId = DeskId([3; 16]);

fn context() -> Context {
    Context {
        mouser_version: "0.3.0".into(),
        os: Os::Linux,
        os_version: "Ubuntu 24.04".into(),
        mode: ComputerMode::Headless,
        desk: None,
    }
}

fn lines(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("one JSON record per line"))
        .collect()
}

fn disconnect(log: &Log, n: u8) {
    log.write(events::peer_disconnected(
        ComputerId([n; 16]),
        LinkLossReason::PeerQuit,
    ));
}

#[test]
fn a_new_file_starts_with_the_header() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::with_context(dir.path(), ME, "Alpha".into(), true, 1, context());
    disconnect(&log, 1);
    let written = lines(&log.path());
    assert_eq!(written.len(), 2);
    let validator = validator();
    for record in &written {
        assert!(validator.is_valid(record), "{record}");
    }
    let header = &written[0];
    assert_eq!(header["event"], "log.header");
    assert_eq!(header["level"], "info");
    assert_eq!(header["computer"]["name"], "Alpha");
    let fields = &header["fields"];
    assert_eq!(fields["mouser_version"], "0.3.0");
    assert_eq!(fields["os"], "linux");
    assert_eq!(fields["os_version"], "Ubuntu 24.04");
    assert_eq!(fields["computer_id"], ME.to_string());
    assert_eq!(fields["computer_name"], "Alpha");
    assert_eq!(fields["mode"], "headless");
    assert_eq!(fields["log_path"], log.path().to_string_lossy().as_ref());
    assert!(fields.get("desk_id").is_none());
    assert_eq!(written[1]["event"], "peer.disconnected");
}

#[test]
fn turning_logging_on_writes_the_header_and_then_says_so() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::with_context(dir.path(), ME, "Alpha".into(), false, 1, context());
    assert!(!log.path().exists());
    log.set_desk(Some((DESK, "Home office".into())));
    log.apply(true, 1, "Desk Mac");
    let written = lines(&log.path());
    assert_eq!(written[0]["event"], "log.header");
    assert_eq!(written[0]["computer"]["name"], "Desk Mac");
    assert_eq!(written[0]["fields"]["computer_name"], "Desk Mac");
    assert_eq!(written[0]["fields"]["desk_id"], DESK.to_string());
    assert_eq!(written[0]["fields"]["desk_name"], "Home office");
    assert_eq!(written[1]["event"], "logging.enabled");
    assert_eq!(
        written[1]["fields"]["path"],
        log.path().to_string_lossy().as_ref()
    );
}

#[test]
fn a_file_opened_again_after_a_restart_gets_a_header_of_the_new_run() {
    let dir = tempfile::tempdir().unwrap();
    let first = Log::with_context(dir.path(), ME, "Alpha".into(), true, 1, context());
    disconnect(&first, 1);
    let second = Log::with_context(
        dir.path(),
        ME,
        "Alpha".into(),
        true,
        1,
        Context {
            mouser_version: "0.3.1".into(),
            ..context()
        },
    );
    disconnect(&second, 2);
    let events: Vec<_> = lines(&second.path())
        .iter()
        .map(|r| {
            (
                r["event"].as_str().unwrap().to_string(),
                r["fields"]["mouser_version"].clone(),
            )
        })
        .collect();
    assert_eq!(
        events,
        [
            ("log.header".to_string(), Value::from("0.3.0")),
            ("peer.disconnected".to_string(), Value::Null),
            ("log.header".to_string(), Value::from("0.3.1")),
            ("peer.disconnected".to_string(), Value::Null),
        ]
    );
}

#[test]
fn the_file_a_rotation_starts_begins_with_the_header_and_the_current_desk() {
    let dir = tempfile::tempdir().unwrap();
    // A name this long makes every record 100 KB, so a 512 KiB file fills
    // after a few of them.
    let name = "A".repeat(100_000);
    let log = Log::with_context(dir.path(), ME, name.clone(), true, 1, context());
    log.set_desk(Some((DESK, "Home office".into())));
    for n in 0..12 {
        disconnect(&log, n);
    }
    let rotated = log.path().with_extension("log.1");
    assert!(rotated.exists());
    let validator = validator();
    for file in [log.path(), rotated] {
        let written = lines(&file);
        assert_eq!(written[0]["event"], "log.header", "{}", file.display());
        assert!(validator.is_valid(&written[0]));
    }
    // The second file was opened after the desk was known.
    let current = lines(&log.path());
    assert_eq!(current[0]["fields"]["desk_name"], "Home office");
    assert_eq!(current[0]["fields"]["computer_name"], name.as_str());
}

#[test]
fn without_a_host_context_the_header_describes_this_build() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "Alpha".into(), true, 1);
    let written = lines(&log.path());
    assert_eq!(written[0]["event"], "log.header");
    assert_eq!(
        written[0]["fields"]["mouser_version"],
        env!("CARGO_PKG_VERSION")
    );
    assert!(validator().is_valid(&written[0]));
}
