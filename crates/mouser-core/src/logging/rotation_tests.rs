//! Two files, each at most `max_log_mb / 2` MiB, so together they never
//! exceed `max_log_mb` (FR-008).

use mouser_proto::ids::ComputerId;
use serde_json::Value;

use super::Log;
use super::events::{self, LinkLossReason};

const ME: ComputerId = ComputerId([7; 16]);

fn lines(path: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("one JSON record per line"))
        .collect()
}

fn size(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).unwrap().len()
}

/// Record number `n`: the peer is named by it.
fn record(log: &Log, n: u8) {
    log.write(events::peer_disconnected(
        ComputerId([n; 16]),
        LinkLossReason::PeerLeft,
    ));
}

fn peers(path: &std::path::Path) -> Vec<String> {
    lines(path)
        .iter()
        .filter(|r| r["event"] == "peer.disconnected")
        .map(|r| r["peer"].as_str().unwrap().to_string())
        .collect()
}

const HALF_MIB: u64 = 512 * 1024;

#[test]
fn mouser_log_rotates_to_mouser_log_1_when_the_next_record_would_exceed_half_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    // 100 KB per record (the computer's name is in each), 200 KB for the
    // header (it names the computer twice).
    let log = Log::new(dir.path(), ME, "A".repeat(100_000), true, 1);
    let rotated = log.path().with_extension("log.1");
    // The header and three records are 500 KB: the next would pass 512 KiB.
    for n in 0..3 {
        record(&log, n);
    }
    assert!(!rotated.exists(), "{} bytes", size(&log.path()));
    record(&log, 3);
    assert!(rotated.exists());
    assert_eq!(peers(&rotated).len(), 3);
    assert_eq!(peers(&log.path()), [ComputerId([3; 16]).to_string()]);
    assert!(size(&rotated) <= HALF_MIB);
}

#[test]
fn the_previous_mouser_log_1_is_deleted_and_the_two_files_never_exceed_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "A".repeat(100_000), true, 1);
    let rotated = log.path().with_extension("log.1");
    let mut oldest_in_rotated = Vec::new();
    for n in 0..40 {
        record(&log, n);
        assert!(
            size(&log.path()) <= HALF_MIB,
            "record {n}: {}",
            size(&log.path())
        );
        if rotated.exists() {
            assert!(size(&rotated) <= HALF_MIB);
            assert!(size(&log.path()) + size(&rotated) <= 1024 * 1024);
            oldest_in_rotated.push(peers(&rotated)[0].clone());
        }
    }
    // Each rotation replaced the file before: the first record is gone.
    oldest_in_rotated.dedup();
    assert!(oldest_in_rotated.len() >= 3, "{oldest_in_rotated:?}");
    assert!(!peers(&rotated).contains(&ComputerId([0; 16]).to_string()));
    assert!(!peers(&log.path()).contains(&ComputerId([0; 16]).to_string()));
}

#[test]
fn the_limit_follows_the_config() {
    let dir = tempfile::tempdir().unwrap();
    let log = Log::new(dir.path(), ME, "A".repeat(100_000), true, 4);
    for n in 0..12 {
        record(&log, n);
    }
    assert!(!log.path().with_extension("log.1").exists());
    log.apply(true, 1, &"A".repeat(100_000));
    record(&log, 12);
    assert!(log.path().with_extension("log.1").exists());
}

#[test]
fn a_named_log_rotates_to_its_own_name_with_1_under_the_same_rule() {
    let dir = tempfile::tempdir().unwrap();
    let context = super::Context {
        mode: mouser_proto::wire::ComputerMode::Service,
        ..super::Context::fallback()
    };
    let log = Log::named(
        dir.path(),
        "service.log",
        ME,
        "A".repeat(100_000),
        true,
        1,
        context,
    );
    let logs = dir.path().join("logs");
    assert_eq!(log.path(), logs.join("service.log"));
    for n in 0..4 {
        record(&log, n);
    }
    let rotated = logs.join("service.log.1");
    assert_eq!(peers(&rotated).len(), 3);
    assert_eq!(peers(&log.path()), [ComputerId([3; 16]).to_string()]);
    assert!(!logs.join("mouser.log").exists());
    for file in [log.path(), rotated] {
        let first = &lines(&file)[0];
        assert_eq!(first["event"], "log.header");
        assert_eq!(first["fields"]["mode"], "service");
    }
}
