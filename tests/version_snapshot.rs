use std::process::Command;

fn binary_path() -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("target");
    path.push(if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    });
    path.push(if cfg!(windows) {
        "decibri.exe"
    } else {
        "decibri"
    });
    path
}

fn run_version(args: &[&str]) -> String {
    let output = Command::new(binary_path())
        .args(args)
        .output()
        .expect("failed to execute decibri binary — run `cargo build` first");
    assert!(
        output.status.success(),
        "decibri exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("non-utf8 output")
}

#[test]
fn version_json_schema_locked() {
    let stdout = run_version(&["version", "--json"]);
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("version --json must be valid JSON");

    let obj = parsed
        .as_object()
        .expect("top-level JSON must be an object");

    let expected_fields = [
        "decibri_cli",
        "decibri",
        "audio_backend",
        "target",
        "rust_version",
    ];
    let actual_fields: std::collections::BTreeSet<&str> = obj.keys().map(String::as_str).collect();
    let expected_set: std::collections::BTreeSet<&str> = expected_fields.iter().copied().collect();
    assert_eq!(
        actual_fields, expected_set,
        "version --json schema drifted; locked at v0.1.0"
    );

    for field in expected_fields {
        assert!(
            obj.get(field)
                .and_then(|v| v.as_str())
                .is_some_and(|s| !s.is_empty()),
            "field `{field}` must be a non-empty string"
        );
    }

    insta::with_settings!({
        sort_maps => true,
    }, {
        insta::assert_json_snapshot!(parsed, {
            ".audio_backend" => "[backend]",
            ".target" => "[target]",
            ".rust_version" => "[rust_version]",
        });
    });
}

#[test]
fn version_human_output_shape() {
    let stdout = run_version(&["version"]);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.len(),
        5,
        "human output must be exactly 5 lines, got: {stdout:?}"
    );
    assert!(
        lines[0].starts_with("decibri-cli "),
        "line 1: {:?}",
        lines[0]
    );
    assert!(lines[1].starts_with("decibri "), "line 2: {:?}", lines[1]);
    assert!(
        lines[2].starts_with("Audio backend: "),
        "line 3: {:?}",
        lines[2]
    );
    assert!(lines[3].starts_with("Platform: "), "line 4: {:?}", lines[3]);
    assert!(lines[4].starts_with("Rust: "), "line 5: {:?}", lines[4]);
}

#[test]
fn version_flag_still_works() {
    let stdout = run_version(&["--version"]);
    assert!(
        stdout.starts_with("decibri "),
        "clap --version flag broken: {stdout:?}"
    );
}
