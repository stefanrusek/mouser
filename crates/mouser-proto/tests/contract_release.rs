use mouser_proto::release::ReleaseManifest;

const LATEST: &str = r#"{
  "version": "v1.2.3-rc.1",
  "notes": "Fixes",
  "pub_date": "2026-05-01T12:00:00Z",
  "min_protocol": 1,
  "platforms": {
    "darwin-aarch64": {"url": "https://example.com/a.tar.gz", "signature": "c2ln"},
    "windows-x86_64-nsis": {"url": "https://example.com/b.zip", "signature": "c2ln2"}
  }
}"#;

fn validator() -> jsonschema::Validator {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../schemas/release-manifest.schema.json")).unwrap();
    jsonschema::validator_for(&schema).unwrap()
}

#[test]
fn sample_latest_json_validates_and_parses() {
    let value: serde_json::Value = serde_json::from_str(LATEST).unwrap();
    assert!(validator().is_valid(&value));
    let m: ReleaseManifest = serde_json::from_str(LATEST).unwrap();
    assert_eq!(m.version, "v1.2.3-rc.1");
    assert_eq!(m.min_protocol, Some(1));
    assert_eq!(m.platforms["darwin-aarch64"].signature, "c2ln");
    assert_eq!(m.notes.as_deref(), Some("Fixes"));
    assert_eq!(m.pub_date.as_deref(), Some("2026-05-01T12:00:00Z"));
}

#[test]
fn reserialized_manifest_still_validates_and_omits_absent_fields() {
    let min = r#"{"version":"1.0.0","platforms":{}}"#;
    let m: ReleaseManifest = serde_json::from_str(min).unwrap();
    let out = serde_json::to_value(&m).unwrap();
    assert_eq!(out, serde_json::from_str::<serde_json::Value>(min).unwrap());
    assert!(validator().is_valid(&out));
    let full: ReleaseManifest = serde_json::from_str(LATEST).unwrap();
    assert!(validator().is_valid(&serde_json::to_value(&full).unwrap()));
}
