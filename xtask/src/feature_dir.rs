use std::path::{Path, PathBuf};

const FEATURE_JSON: &str = ".specify/feature.json";

pub fn evidence_dir() -> PathBuf {
    evidence_dir_from(Path::new(FEATURE_JSON))
}

fn evidence_dir_from(feature_json: &Path) -> PathBuf {
    let text = std::fs::read_to_string(feature_json)
        .unwrap_or_else(|e| panic!("cannot read {FEATURE_JSON}: {e}"));
    let value: serde_json::Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{FEATURE_JSON} is not JSON: {e}"));
    let dir = value["feature_directory"]
        .as_str()
        .unwrap_or_else(|| panic!("{FEATURE_JSON} has no string key feature_directory"));
    Path::new(dir).join("evidence")
}

#[cfg(test)]
#[path = "feature_dir_tests.rs"]
mod tests;
