use std::fs;

fn main() {
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=build.rs");

    let lock = fs::read_to_string("Cargo.lock")
        .expect("Cargo.lock not found; run `cargo generate-lockfile` first");

    let version = extract_decibri_version(&lock)
        .unwrap_or_else(|| panic!("decibri not found in Cargo.lock; build environment is broken"));

    println!("cargo:rustc-env=DECIBRI_VERSION={version}");

    let target = std::env::var("TARGET").expect("TARGET env var not set by cargo");
    println!("cargo:rustc-env=TARGET_TRIPLE={target}");
}

fn extract_decibri_version(lock: &str) -> Option<String> {
    let mut in_decibri = false;
    for line in lock.lines() {
        let trimmed = line.trim();
        if trimmed == "name = \"decibri\"" {
            in_decibri = true;
            continue;
        }
        if in_decibri && trimmed.starts_with("version = \"") {
            return trimmed
                .strip_prefix("version = \"")
                .and_then(|s| s.strip_suffix('"'))
                .map(|s| s.to_string());
        }
        if trimmed.starts_with("name = \"") {
            in_decibri = false;
        }
    }
    None
}
