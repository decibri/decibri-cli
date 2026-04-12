# Changelog

All notable changes to decibri-cli will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - Unreleased

### Added

- `decibri version` subcommand with human-readable and JSON output modes. The `version --json` schema is locked at v0.1.0: `{decibri_cli, decibri, audio_backend, target, rust_version}`.
- `decibri devices` subcommand listing audio input and output devices as a table or as JSON. Supports `--input`, `--output`, and `--json` flags.
- `decibri capture` subcommand for WAV recording from a microphone.
  - Flags: `--output`, `--duration`, `--rate`, `--channels`, `--device`.
  - Device selection by case-insensitive name substring or numeric index.
  - Default configuration is 16000 Hz mono (voice/ASR preset); `--rate 44100 --channels 2` is the music preset.
  - Duration accepts bare seconds (`5`, `10.5`) or humantime strings (`10s`, `1m30s`).
  - Records until the specified duration or until Ctrl+C.
  - Ctrl+C produces a valid truncated WAV, not a corrupt file.
  - Watchdog protection: if the disk writer falls more than ~16 seconds behind, capture stops cleanly with exit 4 and the partial recording is preserved.
  - Clean device-unplug handling: loss mid-capture exits 4 with a partial WAV.
  - Output format is always 16-bit PCM WAV (universally compatible).
- `decibri play` subcommand for WAV file playback.
  - Flags: `<FILE>` (positional), `--device`.
  - Supports 16-bit PCM int and 32-bit float WAV inputs.
  - Unsupported formats (24-bit, 8-bit, non-PCM codecs) exit 1 with a clear error.
  - Ctrl+C mid-playback exits 0 with `"interrupted": true` in JSON output.
- Global flags `--json` and `--quiet` on all subcommands.
- Hidden `decibri completions <shell>` plumbing via `clap_complete`. Shell completion generation ships publicly in v0.3.0; the subcommand is wired now so it is an additive change later.
- Documented exit code table: 0 success, 1 generic error, 2 invalid arguments, 3 device not found, 4 IO error.
- Cross-platform release pipeline building Windows x86_64, Linux x86_64, Linux aarch64, macOS x86_64, macOS aarch64, and a macOS universal2 binary.
- npm distribution via `npm install -g decibri-cli`. The postinstall script downloads the platform binary from the matching GitHub Release, verifies its SHA256 against the release manifest, and places it on the user's PATH.
- SLSA provenance attestations on every release binary via GitHub Actions.
- `SHA256SUMS` manifest attached to every release for integrity verification.

[Unreleased]: https://github.com/decibri/decibri-cli/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/decibri/decibri-cli/releases/tag/v0.1.0
