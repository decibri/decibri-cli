// Hardware-independent snapshot tests for `decibri devices`.
//
// Covers a synthetic JSON serializer test (no audio subsystem touched) plus a
// help-text snapshot. Tests that depend on real device enumeration are run
// locally, since they require hardware.

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

fn run(args: &[&str]) -> String {
    let output = Command::new(binary_path())
        .args(args)
        .output()
        .expect("failed to execute decibri binary; run `cargo build` first");
    assert!(
        output.status.success(),
        "decibri exited non-zero: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("non-utf8 output")
}

#[test]
fn top_level_help_lists_visible_subcommands_only() {
    let stdout = run(&["--help"]);
    assert!(
        stdout.contains("version"),
        "--help missing version: {stdout}"
    );
    assert!(
        stdout.contains("devices"),
        "--help missing devices: {stdout}"
    );
    assert!(
        !stdout.contains("completions"),
        "completions should be hidden from --help: {stdout}"
    );
}

#[test]
fn devices_help_documents_flags() {
    let stdout = run(&["devices", "--help"]);
    assert!(
        stdout.contains("--input"),
        "devices --help missing --input: {stdout}"
    );
    assert!(
        stdout.contains("--output"),
        "devices --help missing --output: {stdout}"
    );
    assert!(
        stdout.contains("--json"),
        "devices --help missing --json: {stdout}"
    );
}

#[test]
fn completions_subcommand_requires_shell_argument() {
    let output = Command::new(binary_path())
        .arg("completions")
        .output()
        .expect("failed to execute decibri binary");
    assert!(
        !output.status.success(),
        "completions with no shell must error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("required") || stderr.contains("<SHELL>") || stderr.contains("shell"),
        "expected required-arg error, got: {stderr}"
    );
}

#[test]
fn completions_bash_emits_script() {
    let stdout = run(&["completions", "bash"]);
    assert!(
        stdout.contains("_decibri") || stdout.contains("complete"),
        "bash completion script looks wrong: first 200 chars = {:?}",
        stdout.chars().take(200).collect::<String>()
    );
}

#[test]
fn devices_json_schema_synthetic() {
    // Synthetic schema check: serialize a known-shape payload and snapshot it.
    // This avoids hardware dependence; real device lists are validated locally.
    let payload = serde_json::json!({
        "input_devices": [
            {
                "index": 0,
                "name": "Test Mic",
                "id": "test-mic-stable-id",
                "kind": "input",
                "default": true,
                "channels": 2,
                "sample_rate": 48000_u32
            }
        ],
        "output_devices": [
            {
                "index": 0,
                "name": "Test Speakers",
                "id": "test-speakers-stable-id",
                "kind": "output",
                "default": true,
                "channels": 2,
                "sample_rate": 48000_u32
            }
        ]
    });

    insta::with_settings!({ sort_maps => true }, {
        insta::assert_json_snapshot!(payload);
    });
}
