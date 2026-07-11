# decibri-cli

Cross-platform CLI for audio capture, playback, and device management.
One binary, zero runtime dependencies, scriptable from any shell.

## Install

```
npm install -g decibri-cli
```

## Quick start

```
decibri capture -o recording.wav -d 10    # Record 10 seconds
decibri play recording.wav                 # Play it back
decibri devices                            # List audio devices
decibri version                            # Show version info
```

## Supported platforms

- Windows x64
- Linux x64
- Linux arm64
- macOS (Apple Silicon and Intel, via universal2 binary)

## How it works

The npm package is a thin wrapper. On install, a postinstall script
downloads the platform-appropriate binary from the GitHub Release matching
this package version, verifies its SHA256, and places it on your PATH. No
Node.js is needed at runtime; once installed, `decibri` is a standalone
binary.

## Links

- Full documentation: https://decibri.com/cli
- GitHub: https://github.com/decibri/decibri-cli
- Report issues: https://github.com/decibri/decibri-cli/issues

## License

Apache-2.0
