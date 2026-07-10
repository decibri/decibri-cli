# decibri-cli

[![npm](https://img.shields.io/npm/v/decibri-cli.svg)](https://www.npmjs.com/package/decibri-cli)
[![crates.io](https://img.shields.io/crates/v/decibri-cli.svg)](https://crates.io/crates/decibri-cli)
[![CI](https://github.com/decibri/decibri-cli/actions/workflows/ci.yml/badge.svg)](https://github.com/decibri/decibri-cli/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

Cross-platform CLI for audio capture, playback, and device management. One binary, zero runtime dependencies, scriptable from any shell.

## Why decibri-cli?

Recording and playing audio from a shell script should be easy. In practice, the existing options are rough: **SoX** is barely maintained and painful on Windows; **arecord** and **aplay** are Linux-only; **ffmpeg** works but is a 70 MB dependency for a task that should be 500 KB.

`decibri-cli` is a modern alternative focused on one thing: scriptable audio I/O. Capture from a microphone, play a WAV, list devices. That's it. The binary is ~850 KB, runs on Windows, Linux, and macOS from the same command line, and produces standard 16-bit PCM WAV files that every other audio tool understands. It's built on the [`decibri`](https://github.com/decibri/decibri) Rust audio library, which uses [cpal](https://github.com/RustAudio/cpal) for native audio I/O on every supported platform — no JACK, PulseAudio, or virtualenv dance required.

Common jobs it handles cleanly: ASR pipeline inputs, CI audio diagnostics, quick voice recordings for debugging, round-trip tests for audio drivers, and anywhere else you need a one-liner that "records this, plays that, lists those."

## Install

### npm (recommended)

```
npm install -g decibri-cli
```

The npm wrapper downloads the platform binary from the matching GitHub Release, verifies its SHA256 against a signed checksum file, and places it on your PATH. No Node.js is needed at runtime.

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

- `--json` — emit machine-readable JSON output where supported
- `--quiet`, `-q` — suppress non-essential human output (progress bars, status messages)

### `decibri version`

Show version and build information.

```
$ decibri version
decibri-cli 0.1.0
decibri 3.0.0
Audio backend: WASAPI
Platform: x86_64-pc-windows-msvc
Rust: 1.82.0
```

The `--json` output schema is **stable** from v0.1.0:

```json
{
  "decibri_cli": "0.1.0",
  "decibri": "3.0.0",
  "audio_backend": "WASAPI",
  "target": "x86_64-pc-windows-msvc",
  "rust_version": "1.82.0"
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
| `--channels <N>` | `-c` | `1` | 1 = mono, 2 = stereo |
| `--device <NAME_OR_INDEX>` | | default input | Device name substring (case-insensitive) or numeric index from `decibri devices` |

Output is always 16-bit PCM WAV. Ctrl+C produces a valid truncated WAV, not a corrupt file. Long recordings have stable memory usage — a 60-minute capture does not grow RSS unboundedly.

Examples:

```
# Voice recording for ASR (default settings are already right)
decibri capture -o speech.wav -d 30

# Music recording: 44.1 kHz stereo
decibri capture -o song.wav -d 60 -r 44100 -c 2

# Record from a specific microphone by name substring
decibri capture -o yeti.wav -d 10 --device "yeti"

# Record from device index 2
decibri capture -o mic2.wav -d 10 --device 2

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

Name matching is case-insensitive substring — `"yeti"` matches `"Blue Yeti USB Microphone"`.

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
| 4 | IO error (file not found, disk full, permission denied, audio device lost mid-capture) |

## Supported platforms

| Platform | Architecture | Distribution |
|---|---|---|
| Windows | x86_64 | npm, direct download |
| Linux | x86_64 | npm, direct download, `cargo install` |
| Linux | aarch64 | npm, direct download, `cargo install` |
| macOS | Intel + Apple Silicon | npm, direct download (universal2 binary) |

## Comparison with other tools

| | decibri-cli | SoX | ffmpeg | arecord/aplay |
|---|---|---|---|---|
| Cross-platform | ✅ | ⚠️ painful on Windows | ✅ | ❌ Linux only |
| Zero runtime deps | ✅ (one binary) | ⚠️ | ❌ (70+ MB) | ✅ |
| Actively maintained | ✅ | ❌ | ✅ | ✅ |
| Modern CLI syntax | ✅ | ❌ (cryptic) | ❌ (complex) | ⚠️ ALSA-specific |
| Binary size | ~850 KB | ~4 MB | ~70 MB | N/A (system) |
| Install via npm | ✅ | ❌ | ❌ | ❌ |
| JSON output for scripting | ✅ | ❌ | ⚠️ (ffprobe) | ❌ |

`decibri-cli` is not trying to replace ffmpeg. If you need transcoding, filtering, mixing, or any non-trivial audio processing, use ffmpeg. `decibri-cli` is for the cases where ffmpeg is too much: scripting a capture, playing a WAV, listing devices, checking that your mic works.

## How it works

`decibri-cli` is a thin shell over the [`decibri`](https://github.com/decibri/decibri) Rust audio library. Device enumeration and cpal streams come from the library; the CLI adds argument parsing (clap), WAV I/O (hound), progress bars (indicatif), and the exit-code table. The release binary is compiled with `opt-level = "z"`, link-time optimization, and `panic = "abort"` — the standard Rust size-shrinking profile. Default decibri features are trimmed to `capture` and `output` only (VAD and denoise are v0.2.0 features), which keeps the binary under 1 MB.

Capture is synchronous and callback-based under the hood. A dedicated worker thread receives audio chunks from cpal via the library's internal channel and streams them into a `hound::WavWriter` with backpressure protection — a watchdog stops the stream cleanly if the writer falls more than ~16 seconds behind, so disk stalls don't balloon memory. Ctrl+C triggers a cooperative shutdown that drains the last chunks, finalizes the WAV header, and exits with code 0.

## Security notes

**Windows SmartScreen.** v0.1.x binaries are unsigned. First run may show a SmartScreen warning; click **More info** → **Run anyway**. This is a known limitation and will be revisited for v0.2.0 based on user demand for EV certificates.

**macOS Gatekeeper.** Direct downloads on macOS may trigger "cannot be opened because the developer cannot be verified." Remove the quarantine flag:

```
xattr -d com.apple.quarantine /path/to/decibri
```

The `npm install -g` path bypasses this because npm writes the binary through a different code path than the browser download. Homebrew-style distribution (v0.3.0) will include proper notarization.

**Binary provenance.** Every release binary is built via GitHub Actions with SLSA provenance attestations. Verify any download with the GitHub CLI:

```
gh attestation verify decibri-x86_64-unknown-linux-gnu.tar.gz --owner decibri
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

## Roadmap

### v0.1.0 (current)
- Audio capture to WAV (`decibri capture`)
- WAV file playback (`decibri play`)
- Device listing (`decibri devices`)
- Version and build metadata (`decibri version`)
- Cross-platform distribution (npm, crates.io, direct download)

### v0.2.0 (planned)
- Voice activity detection (`decibri capture --vad`)
- Raw PCM piping via stdin/stdout (`decibri capture --raw`, `decibri play --raw`)
- Diagnostics subcommand (`decibri test`)
- Shell completions generation via the existing hidden `completions` plumbing

### v0.3.0+ (future)
- Homebrew formula and Scoop manifest
- Linux package submissions (AUR, etc.)
- Config file support (`~/.decibri/config.toml`)
- Format conversion (`decibri convert`)

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Bug reports, feature requests, and pull requests are all welcome.

## Security

See [SECURITY.md](SECURITY.md). Security issues should be reported privately via GitHub's vulnerability reporting, not as public issues.

## License

Apache-2.0. See [LICENSE](LICENSE).

## Part of the decibri ecosystem

- [`decibri`](https://github.com/decibri/decibri) - the underlying Rust audio library + Node.js bindings
- [`mcp-listen`](https://github.com/decibri/mcp-listen) - MCP server for AI agent voice input
- [`mcp-speak`](https://github.com/decibri/mcp-speak) - MCP server for AI agent voice output

Learn more at [decibri.com](https://decibri.com).
