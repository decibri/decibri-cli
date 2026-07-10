<!-- markdownlint-disable MD024 -->

# Changelog

All notable changes to decibri-cli will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-07-11

### Added

- `--device-id` flag on `capture` and `play`: select a device by its stable per-host ID (exact match). Mutually exclusive with `--device`.
- `id` field in `devices --json` output: the stable per-host device ID (WASAPI endpoint ID on Windows, CoreAudio UID on macOS, ALSA pcm_id on Linux). Empty string when the host cannot assign one.

### Changed

- decibri library updated from 3.0.0 to 5.0.0.
- Capture output now honors `--rate` on every device: the device is opened at its native rate and the library resamples to the requested rate, so a 48 kHz microphone recorded with `--rate 16000` produces a correct 16 kHz WAV.
- `--channels` on `capture` accepts only `1`; capture is mono only. Any other value is rejected as an argument error (exit 2).
- `dropped_chunks` in the `capture --json` completion payload reports the real count of capture buffers dropped while the writer could not keep up (previously always `0`). A nonzero count also prints a stderr warning. Capture completes rather than aborting when the writer falls behind.
- Device loss is reported with the underlying failure from the library: capture exits 4 with the partial recording preserved, and playback cut short by an output-device failure exits 4 instead of reporting success.
- Minimum supported Rust version raised to 1.88.

### Removed

- The writer-lag watchdog that stopped capture with exit 4 when buffering exceeded about 16 seconds. The library now bounds its capture buffer and drops the newest audio when the consumer stalls; the drop count is reported via `dropped_chunks`.

## [0.1.0] - 2026-04-12

Stable release of the same feature set as `0.1.0-alpha.1`. No functional changes since alpha; shipped to the default `latest` npm tag after alpha round-trip testing succeeded on Windows, macOS, and Linux.

## [0.1.0-alpha.1] - 2026-04-12

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

[Unreleased]: https://github.com/decibri/decibri-cli/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/decibri/decibri-cli/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/decibri/decibri-cli/releases/tag/v0.1.0
[0.1.0-alpha.1]: https://github.com/decibri/decibri-cli/releases/tag/v0.1.0-alpha.1
