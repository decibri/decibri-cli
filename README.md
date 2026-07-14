
<p align="center">
  <a href="https://decibri.com">
    <img
      src="https://github.com/user-attachments/assets/bc2c37ed-5105-4007-9efb-c07c6b4a25ac"
      alt="Decibri mcp-listen"
      width="100%">
  </a>
</p>

# decibri-cli

[![npm](https://img.shields.io/npm/v/decibri-cli.svg)](https://www.npmjs.com/package/decibri-cli)
[![crates.io](https://img.shields.io/crates/v/decibri-cli.svg)](https://crates.io/crates/decibri-cli)
[![CI](https://github.com/decibri/decibri-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/decibri/decibri-cli/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Cross-platform CLI for audio capture, playback, and device management. One binary, zero runtime dependencies, scriptable from any shell.

## Why decibri-cli?

Recording and playing audio from a shell script should be simple. decibri-cli is one small binary focused on a single job: scriptable audio I/O. Capture from a microphone, play a WAV, list devices. That is it.

The binary is about 850 KB, runs on Windows, Linux, and macOS from the same command line, and produces standard 16-bit PCM WAV files that every other audio tool understands. It is built on the [`decibri`](https://github.com/decibri/decibri) Rust audio library, which uses [cpal](https://github.com/RustAudio/cpal) for native audio I/O on every supported platform, with no runtime dependencies.

Common jobs it handles cleanly: ASR pipeline inputs, CI audio diagnostics, quick voice recordings for debugging, round-trip tests for audio drivers, and anywhere else you need a one-liner that records this, plays that, lists those.

## Install

### npm (recommended)

```
npm install -g decibri-cli
```

The npm wrapper downloads the platform binary from the matching GitHub Release, verifies its SHA256 against the release's checksum manifest, and places it on your PATH. No Node.js is needed at runtime.

### Cargo

```
cargo install decibri-cli
```

Builds from source. Requires Rust stable and, on Linux, `libasound2-dev` (`sudo apt-get install libasound2-dev`).

### Direct download

Download the archive for your platform from the [Releases page](https://github.com/decibri/decibri-cli/releases), extract it, and place the `decibri` binary on your PATH.

## Quick start

```
# Record 10 seconds to a WAV file
decibri capture -o recording.wav -d 10

# Play it back
decibri play recording.wav

# List audio devices
decibri devices

# Show version info
decibri version
```

## Commands

All commands accept two global flags:

- `--json`: emit machine-readable JSON output where supported
- `--quiet`, `-q`: suppress non-essential human output (progress bars, status messages)

### `decibri version`

Show version and build information.

```
$ decibri version
decibri-cli 0.2.0
decibri 5.0.0
Audio backend: WASAPI
Platform: x86_64-pc-windows-msvc
Rust: 1.88
```

The `--json` output schema is **stable** from v0.1.0:

```json
{
  "decibri_cli": "0.2.0",
  "decibri": "5.0.0",
  "audio_backend": "WASAPI",
  "target": "x86_64-pc-windows-msvc",
  "rust_version": "1.88"
}
```

### `decibri devices`

List available audio input and output devices.

| Flag | Description |
|---|---|
| `--input` | List input devices only |
| `--output` | List output devices only |
| `--json` | Machine-readable JSON output (schema unstable until v1.0.0) |

```
$ decibri devices
Input devices:
┌───────┬──────────────────┬──────────┬──────────┬─────────┐
│ Index │ Name             │ Channels │ Rate     │ Default │
├───────┼──────────────────┼──────────┼──────────┼─────────┤
│ 0     │ Microphone       │ 2        │ 48000 Hz │ ✓       │
│ 1     │ Microphone Array │ 2        │ 48000 Hz │         │
└───────┴──────────────────┴──────────┴──────────┴─────────┘

Output devices:
┌───────┬──────────────┬──────────┬──────────┬─────────┐
│ Index │ Name         │ Channels │ Rate     │ Default │
├───────┼──────────────┼──────────┼──────────┼─────────┤
│ 0     │ Speakers     │ 2        │ 48000 Hz │ ✓       │
└───────┴──────────────┴──────────┴──────────┴─────────┘
```

### `decibri capture`

Record audio from an input device to a WAV file.

| Flag | Short | Default | Description |
|---|---|---|---|
| `--output <FILE>` | `-o` | required | Output WAV file path |
| `--duration <TIME>` | `-d` | unset (record until Ctrl+C) | Recording duration (e.g., `10`, `5.5`, `10s`, `1m30s`) |
| `--rate <HZ>` | `-r` | `16000` | Sample rate in Hz |
| `--channels <N>` | `-c` | `1` | Mono only. Values other than 1 are rejected. |
| `--device <NAME_OR_INDEX>` | | default input | Device name substring (case-insensitive) or numeric index from `decibri devices` |
| `--device-id <ID>` | | unset | Exact device id from `decibri devices --json`. Mutually exclusive with `--device`. |
| `--dc-removal` | | off | Remove a constant DC offset from the captured signal |
| `--highpass <HZ>` | | off | Apply a high-pass filter at the given cutoff in Hz (removes low-frequency rumble). Supported cutoffs: 80, 100. |
| `--agc <DBFS>` | | off | Automatic gain control to the given target level in dBFS. Range: -40 to -3 (for example -20). |
| `--limiter <DBFS>` | | off | Peak limiter ceiling in dBFS. Range: -3.0 to 0.0 (for example -1). |

Output is always 16-bit PCM WAV. Ctrl+C produces a valid truncated WAV, not a corrupt file. Long recordings have stable memory usage: a 60-minute capture does not grow RSS unboundedly.

Examples:

```
# Voice recording for ASR (default settings are already right)
decibri capture -o speech.wav -d 30

# Higher sample rate capture
decibri capture -o clip.wav -d 60 -r 44100

# Record from a specific microphone by name substring
decibri capture -o yeti.wav -d 10 --device "yeti"

# Record from device index 2
decibri capture -o mic2.wav -d 10 --device 2

# Clean, leveled capture for ASR input
decibri capture -o speech.wav -r 16000 --highpass 80 --agc -20

# Record until Ctrl+C, suppress progress output
decibri capture -o long.wav --quiet

# JSON metadata on completion, for scripting
decibri capture -o clip.wav -d 5 --json
```

### `decibri play`

Play a WAV file through an output device.

| Flag | Description |
|---|---|
| `<FILE>` (positional) | WAV file to play |
| `--device <NAME_OR_INDEX>` | Device name substring or numeric index from `decibri devices` (output side) |
| `--device-id <ID>` | Exact device id from `decibri devices --json`. Mutually exclusive with `--device`. |

Supports 16-bit PCM int and 32-bit float WAV inputs. Other formats (24-bit, 8-bit, non-PCM codecs) exit with a clear error. Ctrl+C during playback stops cleanly with exit 0; the completion metadata reports `"interrupted": true` in JSON mode.

Examples:

```
# Play a file
decibri play recording.wav

# Play through a specific output device
decibri play song.wav --device "Speakers"

# Play with JSON completion metadata
decibri play clip.wav --json
```

## Recipes

### Record 30 seconds of speech for ASR

```
decibri capture -o speech.wav -d 30
```

Defaults (16 kHz, mono, 16-bit PCM) are already the standard ASR input shape. No configuration needed.

### Capture from a specific microphone

```
decibri devices --input       # find the device name
decibri capture -o out.wav -d 10 --device "Blue Yeti"
```

Name matching is a case-insensitive substring: `"yeti"` matches `"Blue Yeti USB Microphone"`.

### Test an audio device from a shell script

```
decibri capture -o /tmp/ci-test.wav -d 2 --quiet --json
```

`--quiet` suppresses the progress bar and human status lines. `--json` emits one line of completion metadata to stdout on success, with exit code 0. Any error goes to stderr with a non-zero exit code so your script can fail fast.

### Round-trip a recording

```
decibri capture -o test.wav -d 5 && decibri play test.wav
```

## Exit codes

Scripts can rely on these. They are part of the stable CLI contract.

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Generic error (unsupported WAV format, corrupt file, audio subsystem failure) |
| 2 | Invalid arguments (handled by clap) |
| 3 | Device not found (`--device` given but no match) |
| 4 | IO error (file not found, disk full, permission denied, audio device lost mid-capture or mid-playback) |

## Supported platforms

| Platform | Architecture | Distribution |
|---|---|---|
| Windows | x86_64 | npm, direct download |
| Linux | x86_64 | npm, direct download, `cargo install` |
| Linux | aarch64 | npm, direct download, `cargo install` |
| macOS | Intel + Apple Silicon | npm, direct download (universal2 binary) |

## How it works

`decibri-cli` is a thin shell over the [`decibri`](https://github.com/decibri/decibri) Rust audio library. Device enumeration and the audio streams come from the library; the CLI adds argument parsing (clap), WAV I/O (hound), progress bars (indicatif), and the exit-code table. The release binary is compiled with `opt-level = "z"`, link-time optimization, and `panic = "abort"`, the standard Rust size-shrinking profile. Default decibri features are trimmed to `capture`, `playback`, and `gain` only, which keeps the binary small (`gain` is pure DSP and pulls no dependencies).

Capture pulls requested-rate audio from the library a block at a time and streams it into a `hound::WavWriter`. The device opens at its native rate and the library resamples to the rate you ask for with `--rate`, so the output file always matches the requested rate. If the writer cannot keep up, the library drops the newest audio rather than growing memory without bound; the number of dropped blocks is reported as `dropped_chunks`, and a warning is printed to stderr when it is nonzero, so a long capture on a slow disk completes with an accurate record of any loss instead of failing. Ctrl+C triggers a cooperative shutdown that drains the remaining audio, finalizes the WAV header, and exits with code 0.

## Security notes

**Windows SmartScreen.** Release binaries are unsigned. First run may show a SmartScreen warning; click **More info** → **Run anyway**. This is a known limitation.

**macOS Gatekeeper.** Direct downloads on macOS may trigger "cannot be opened because the developer cannot be verified." Remove the quarantine flag:

```
xattr -d com.apple.quarantine /path/to/decibri
```

The `npm install -g` path bypasses this because npm writes the binary through a different code path than the browser download.

**Binary provenance.** Every release binary is built via GitHub Actions with SLSA provenance attestations. The attestation covers the `decibri` binary itself, not the archive it ships in, so extract first, then verify with the GitHub CLI:

```
tar xzf decibri-x86_64-unknown-linux-gnu.tar.gz
gh attestation verify decibri --owner decibri
```

The attestation proves the binary was built by the repo's release workflow from a specific commit, signed via Sigstore and published to the GitHub attestation store.

**Checksum verification.** Each release includes a `SHA256SUMS` file. Verify your download before extracting:

```
# Linux / macOS
sha256sum -c SHA256SUMS --ignore-missing

# Windows PowerShell
Get-FileHash decibri-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

The npm wrapper does this check automatically on every install.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Bug reports, feature requests, and pull requests are all welcome.

## Security

See [SECURITY.md](SECURITY.md). Security issues should be reported privately via GitHub's vulnerability reporting, not as public issues.

## License

Apache-2.0. See [LICENSE](LICENSE).
