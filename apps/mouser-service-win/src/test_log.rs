//! A service log in a folder of its own, for the unit tests.

use std::sync::Arc;

use mouser_core::logging::{Context, Log, SERVICE_FILE};
use mouser_proto::ids::ComputerId;
use mouser_proto::wire::ComputerMode;

use crate::test_dir::TestDir;

pub struct TestLog {
    _dir: TestDir,
    pub log: Arc<Log>,
}

impl TestLog {
    pub fn new() -> Self {
        let dir = TestDir::new();
        let context = Context {
            mode: ComputerMode::Service,
            ..Context::fallback()
        };
        let log = Log::named(
            dir.path(),
            SERVICE_FILE,
            ComputerId([7; 16]),
            "Alpha".into(),
            true,
            8,
            context,
        );
        Self {
            _dir: dir,
            log: Arc::new(log),
        }
    }

    /// The lines written after the header.
    pub fn lines(&self) -> Vec<String> {
        std::fs::read_to_string(self.log.path())
            .unwrap()
            .lines()
            .skip(1)
            .map(str::to_string)
            .collect()
    }

    /// The `event` and `fields` of each line after the header, as
    /// `event fields-json`.
    pub fn events(&self) -> Vec<String> {
        self.lines()
            .iter()
            .map(|line| {
                let event = between(line, "\"event\":\"", "\"");
                let (_, rest) = line.split_once("\"fields\":").unwrap();
                let fields = &rest[..rest.len() - 1];
                format!("{event} {fields}")
            })
            .collect()
    }
}

fn between<'a>(line: &'a str, from: &str, to: &str) -> &'a str {
    let rest = &line[line.find(from).unwrap() + from.len()..];
    &rest[..rest.find(to).unwrap()]
}
