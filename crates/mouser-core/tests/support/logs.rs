//! Reading the diagnostic logs and bundles of the computers in a lab, and
//! checking them against the contract schemas.

use std::io::Read;
use std::path::{Path, PathBuf};

use jsonschema::{Registry, Validator};
use serde_json::Value;

use super::desk::Node;

/// The v2 catalog of events, with the record schema it refers to.
pub fn events_validator() -> Validator {
    let record: Value = serde_json::from_str(include_str!(
        "../../../mouser-proto/schemas/log-record.schema.json"
    ))
    .unwrap();
    let events: Value = serde_json::from_str(include_str!(
        "../../../mouser-proto/schemas/log-events.schema.json"
    ))
    .unwrap();
    let registry = Registry::new()
        .add("https://mouser.local/schemas/log-record.v1.json", record)
        .unwrap()
        .prepare()
        .unwrap();
    jsonschema::options()
        .with_registry(&registry)
        .build(&events)
        .unwrap()
}

/// The manifest of a desk log bundle.
pub fn bundle_validator() -> Validator {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../mouser-proto/schemas/log-bundle.schema.json"
    ))
    .unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

/// The records in one log file; a line that is not whole yet (the engine is
/// writing it) is left out.
pub fn read(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

/// Where a node's log files are.
pub fn files(node: &Node) -> (PathBuf, PathBuf) {
    let logs = node.dir.path().join("logs");
    (logs.join("mouser.log"), logs.join("mouser.log.1"))
}

/// Every record of a node, the older file first.
pub fn records(node: &Node) -> Vec<Value> {
    let (current, older) = files(node);
    let mut all = read(&older);
    all.extend(read(&current));
    all
}

/// Every line of both files must be a record of the catalog.
pub fn assert_every_record_is_in_the_catalog(node: &Node) {
    let validator = events_validator();
    let (current, older) = files(node);
    for file in [older, current] {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for line in text.lines() {
            let record: Value = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{}: not JSON ({e}): {line}", node.name));
            if let Err(e) = validator.validate(&record) {
                panic!("{}: {e}: {record}", node.name);
            }
        }
    }
}

/// The names in a zip, and a way to read each one.
pub struct Zip(zip::ZipArchive<std::fs::File>);

impl Zip {
    pub fn open(path: &Path) -> Self {
        Self(zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap())
    }

    pub fn names(&mut self) -> Vec<String> {
        (0..self.0.len())
            .map(|i| self.0.by_index(i).unwrap().name().unwrap().to_string())
            .collect()
    }

    pub fn bytes(&mut self, name: &str) -> Vec<u8> {
        let mut out = Vec::new();
        self.0.by_name(name).unwrap().read_to_end(&mut out).unwrap();
        out
    }

    pub fn text(&mut self, name: &str) -> String {
        String::from_utf8(self.bytes(name)).unwrap()
    }

    pub fn manifest(&mut self) -> Value {
        serde_json::from_str(&self.text("bundle.json")).unwrap()
    }
}
