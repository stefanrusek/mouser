//! T014/T019: the embedded schema copies are identical to the contract files.

#[test]
fn schema_copies_are_byte_identical_to_contracts() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let specs = root.join("../../specs");
    let sources = [
        ("status", "002-mouser-v2"),
        ("log-events", "002-mouser-v2"),
        ("log-bundle", "002-mouser-v2"),
        ("log-record", "001-mouser-v1"),
        ("release-manifest", "001-mouser-v1"),
    ];
    for (name, feature) in sources {
        let file = format!("{name}.schema.json");
        let copy = std::fs::read(root.join("schemas").join(&file)).unwrap();
        let original = std::fs::read(specs.join(feature).join("contracts").join(&file)).unwrap();
        assert_eq!(copy, original, "{file}");
    }
}
