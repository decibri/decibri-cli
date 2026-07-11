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

Please open an issue describing your use case before submitting a large feature PR, so we can align on scope first.

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

### What we don't accept

- Large new features or new audio formats beyond WAV without a prior issue and scope discussion
- Breaking changes to the CLI contract (subcommand names, flag names, exit codes, `version --json` schema) until v1.0.0
- Adding async runtimes (`tokio`, `async-std`): the CLI is intentionally sync
- Adding `env_logger` / `tracing`: stderr output is UI, not logs

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

The crate version in `Cargo.toml` and the npm wrapper version in `npm/decibri-cli/package.json` must match; a test in `tests/version_parity.rs` fails `cargo test` if they diverge.

### Snapshot tests

Some tests use [`insta`](https://insta.rs) for snapshot assertions. If you add a test or change snapshot-producing output, run:

```
cargo insta review
```

to approve changes after manual inspection. Do **not** blindly accept snapshots with `cargo insta accept`; review them first.

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

## Contributor License Agreement

Before your first contribution can be merged, we ask you to agree to the decibri Contributor License Agreement. It is a one-time step that lets the project include your work under its current and future licenses, with clear provenance, and it does not take away your copyright in what you contribute. You are welcome to read the full agreements first: the [Individual CLA](https://github.com/decibri/decibri-cla-action/blob/main/agreements/Individual-CLA-v1.md) and, for contributions made on behalf of a company, the [Corporate CLA](https://github.com/decibri/decibri-cla-action/blob/main/agreements/Corporate-CLA-v1.md).

When you open a pull request, an automated check looks at whether you are already covered. If you are not, it leaves a comment with a short sentence to agree to. Reply with that exact sentence as a comment on your own pull request, and the check turns green. Until the check passes, the pull request cannot be merged.

If you are contributing as part of your work, your employer may need a Corporate CLA on file instead of an individual one. If that applies, or the check asks about it, contact the maintainers and we will sort it out.

The record we keep is deliberately minimal: your GitHub username and account ID, which version of the agreement you agreed to, and the date. How we handle that information, and how to request its removal, is set out in our [Privacy Policy](https://decibri.com/privacy).

The CLA covers your contributions across the decibri organisation's repositories, so you only need to agree once.

## License

By contributing, you agree that your contributions will be licensed under Apache-2.0.
