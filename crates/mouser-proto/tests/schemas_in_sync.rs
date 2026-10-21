//! T014: the embedded schema copies are identical to the contract files.

#[test]
fn schema_copies_are_byte_identical_to_contracts() {
    let contracts = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../specs/001-mouser-v1/contracts");
    for name in ["status", "log-record", "release-manifest"] {
        let file = format!("{name}.schema.json");
        let copy = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("schemas")
                .join(&file),
        )
        .unwrap();
        let original = std::fs::read(contracts.join(&file)).unwrap();
        assert_eq!(copy, original, "{file}");
    }
}
