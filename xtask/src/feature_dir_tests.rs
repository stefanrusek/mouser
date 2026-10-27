use super::*;
use std::fs;

fn scratch(name: &str, content: Option<&str>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("xtask-feature-dir-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("feature.json");
    if let Some(c) = content {
        fs::write(&path, c).unwrap();
    }
    path
}

#[test]
fn evidence_dir_is_feature_directory_plus_evidence() {
    let p = scratch(
        "ok",
        Some(r#"{"feature_directory": "specs/002-mouser-v2"}"#),
    );
    assert_eq!(
        evidence_dir_from(&p),
        Path::new("specs/002-mouser-v2/evidence")
    );
}

#[test]
#[should_panic(expected = ".specify/feature.json")]
fn missing_file_panics_naming_feature_json() {
    evidence_dir_from(&scratch("missing", None));
}

#[test]
#[should_panic(expected = ".specify/feature.json")]
fn missing_key_panics_naming_feature_json() {
    evidence_dir_from(&scratch("nokey", Some("{}")));
}

#[test]
#[should_panic(expected = ".specify/feature.json")]
fn invalid_json_panics_naming_feature_json() {
    evidence_dir_from(&scratch("badjson", Some("not json")));
}
