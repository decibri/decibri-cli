# Contributing to decibri-cli

Thanks for your interest in contributing. This guide covers what you need to know.

## Code of conduct

Be respectful. Disagree with ideas, not people. No harassment.

## How to report bugs

1. Check the [issue tracker](https://github.com/decibri/decibri-cli/issues) for duplicates.
2. If the bug is new, open an issue using the Bug Report template.
3. Include:
   - OS and architecture
   - `decibri version` output
   - Steps to reproduce
   - Expected vs. actual behaviour
   - Command output (stderr and stdout)

## How to request features

Open an issue using the Feature Request template. Explain:

- The problem you are trying to solve
- Your proposed solution (if you have one)
- Alternatives you considered

Scope-note: v0.1.x is focused on the core command surface. Features flagged for v0.2.0 (VAD, raw PCM piping, diagnostics) or v0.3.0 (Homebrew/Scoop, config files) already have homes on the roadmap — we'd still appreciate an issue describing your use case, but code contributions for those features should wait until the relevant version milestone is open.

## How to contribute code

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/your-feature`
3. Make your changes
4. Ensure all CI checks pass locally:
   ```
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   cargo deny check           # if you have cargo-deny installed
   cargo audit                # if you have cargo-audit installed
   ```
5. For npm wrapper changes, also run:
   ```
   cd npm/decibri-cli
   node --test tests/platform.test.js tests/sha256sums.test.js tests/version.test.js
   ```
6. Commit with a clear message
7. Push and open a Pull Request

### What we accept

- Bug fixes with tests demonstrating the fix
- Platform compatibility improvements
- Documentation improvements
- Performance improvements with a measurement
- Small refactors that reduce code without changing behaviour

### What we don't accept for v0.1.x

- Features explicitly deferred to v0.2.0 (VAD, raw PCM piping, `decibri test`)
- New audio format support beyond WAV (deferred to v0.2.0+)
- Breaking changes to the CLI contract (subcommand names, flag names, exit codes, `version --json` schema) until v1.0.0
- Adding async runtimes (`tokio`, `async-std`) — the CLI is intentionally sync
- Adding `env_logger` / `tracing` — stderr output is UI, not logs, in v0.1.x

## Development setup

### Requirements

- Rust stable toolchain. MSRV is recorded in `Cargo.toml` under `rust-version`.
- Node.js 18 or newer (for running the npm wrapper tests)
- On Linux, `libasound2-dev` for the cpal backend: `sudo apt-get install libasound2-dev`

### Building

```
cargo build                    # debug build
cargo build --release          # optimised release build
```

The release binary lands at `target/release/decibri` (or `decibri.exe` on Windows).

### Running locally

```
./target/debug/decibri version
./target/debug/decibri devices
./target/debug/decibri capture -o test.wav -d 3
./target/debug/decibri play test.wav
```

### Testing

```
cargo test                                          # all Rust tests
cd npm/decibri-cli && node --test tests/*.test.js   # npm wrapper tests
```

The Rust tests are hardware-independent and run in CI. Manual hardware tests (real microphone / speaker) are local-only because CI runners don't have audio devices.

### Snapshot tests

Some tests use [`insta`](https://insta.rs) for snapshot assertions. If you add a test or change snapshot-producing output, run:

```
cargo insta review
```

to approve changes after manual inspection. Do **not** blindly accept snapshots with `cargo insta accept` — review them first.

## Project structure

```
decibri-cli/
├── src/                       Rust source
│   ├── main.rs                clap entry point + subcommand dispatch
│   ├── exit.rs                exit-code marker types
│   ├── device_resolve.rs      shared --device parsing + resolution
│   └── commands/
│       ├── version.rs
│       ├── devices.rs
│       ├── capture.rs
│       └── play.rs
├── tests/                     integration tests (hardware-independent)
├── npm/decibri-cli/           npm wrapper package
├── .github/
│   ├── workflows/             CI + release workflows
│   ├── ISSUE_TEMPLATE/
│   └── dependabot.yml
├── Cargo.toml
├── deny.toml                  cargo-deny config
├── CHANGELOG.md
└── README.md
```

## License

By contributing, you agree that your contributions will be licensed under Apache-2.0.
