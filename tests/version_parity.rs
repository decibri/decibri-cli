//! Fails `cargo test` when the crate version and the npm wrapper version
//! diverge. Both must be bumped together on every release.

use std::path::Path;

#[test]
fn crate_and_npm_wrapper_versions_match() {
    let package_json_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("npm")
        .join("decibri-cli")
        .join("package.json");

    let contents = std::fs::read_to_string(&package_json_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", package_json_path.display()));

    let package: serde_json::Value =
        serde_json::from_str(&contents).expect("npm/decibri-cli/package.json must be valid JSON");

    let npm_version = package
        .get("version")
        .and_then(|v| v.as_str())
        .expect("npm/decibri-cli/package.json must have a string `version` field");

    let crate_version = env!("CARGO_PKG_VERSION");

    assert_eq!(
        crate_version, npm_version,
        "version mismatch: Cargo.toml is {crate_version} but npm/decibri-cli/package.json is {npm_version}; bump both together"
    );
}
