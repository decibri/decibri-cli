use serde::Serialize;

const DECIBRI_CLI_VERSION: &str = env!("CARGO_PKG_VERSION");
const DECIBRI_VERSION: &str = env!("DECIBRI_VERSION");
const RUST_VERSION: &str = env!("CARGO_PKG_RUST_VERSION");
const TARGET: &str = env!("TARGET_TRIPLE");

// Compile-time mapping because cpal::default_host() is selected by target_os
// with default features. If cpal is ever built with non-default features
// (e.g., JACK on Linux), this mapping will drift.
const AUDIO_BACKEND: &str = if cfg!(target_os = "windows") {
    "WASAPI"
} else if cfg!(target_os = "macos") {
    "CoreAudio"
} else if cfg!(target_os = "linux") {
    "ALSA"
} else {
    "unknown"
};

#[derive(Serialize)]
struct VersionInfo {
    decibri_cli: &'static str,
    decibri: &'static str,
    audio_backend: &'static str,
    target: &'static str,
    rust_version: &'static str,
}

impl VersionInfo {
    const fn current() -> Self {
        Self {
            decibri_cli: DECIBRI_CLI_VERSION,
            decibri: DECIBRI_VERSION,
            audio_backend: AUDIO_BACKEND,
            target: TARGET,
            rust_version: RUST_VERSION,
        }
    }
}

pub fn run(json: bool) -> anyhow::Result<()> {
    let info = VersionInfo::current();
    if json {
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("decibri-cli {}", info.decibri_cli);
        println!("decibri {}", info.decibri);
        println!("Audio backend: {}", info.audio_backend);
        println!("Platform: {}", info.target);
        println!("Rust: {}", info.rust_version);
    }
    Ok(())
}
