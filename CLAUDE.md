# CLAUDE.md — decibri-cli

This file tells Claude Code how to work in this repo. The canonical architectural reference is [BUILD-PLAN.md](./BUILD-PLAN.md). Consult it for the phased work order, dependency list, and decision record before making non-trivial changes.

## Guardrails

**Claude Code must never:**

- Run `git commit`, `git push`, or push to any remote
- Run `npm publish` or `cargo publish`
- Create or push git tags
- Modify files in `.github/workflows/` without explicit approval in the same request

All commits, tags, and registry publishes are performed manually. If a task appears to require any of the above, stop and ask first.

**Claude Code is allowed to:**

- Stage changes with `git add`
- Inspect repo state with `git status`, `git diff`, `git log`
- Run tests, builds, linters, and formatters locally
- Modify any source file outside `.github/workflows/`

## Pre-Phase-0 research gate

Do not start coding until the `decibri` 3.0.0 capture API has been read at `https://docs.rs/decibri/3.0.0/decibri/capture/` and the sync/async question is answered. This determines whether the CLI needs `tokio` and is the single biggest design choice. See BUILD-PLAN.md §2 "Assumptions to verify before coding."

## Code Quality

- Run `cargo fmt --all` before committing.
- Run `cargo clippy --all-targets -- -D warnings` before committing Rust changes. Fix all warnings. Do not suppress with `#[allow]`.
- Run `cargo test` after any Rust changes to verify no regressions.
- Run `cargo deny check` before committing any `Cargo.toml` dependency change.
- Run `cargo audit` periodically. CI runs it on every PR.
- Snapshot tests use `cargo insta`. Accept changes only after manual review with `cargo insta review`. Do not blindly run `cargo insta accept`.
- Do not use em dashes (the long dash character) anywhere in the codebase. Rewrite sentences using periods, commas, colons, or parentheses instead.

## API Compatibility

The CLI's user-facing contract is frozen after v0.1.0 ships. Breaking changes require a major version bump.

- **Subcommand names, flag names, and argument shapes** are the contract. Do not rename `--output` to `--out`, do not change `decibri capture` to `decibri record`, without an explicit major-version plan.
- **Exit codes** are a documented contract. See BUILD-PLAN.md for the exit code table (0 success, 1 generic error, 2 bad args, 3 device not found, 4 IO error). Script users depend on these.
- **`version --json` schema** is locked at v0.1.0 to `{decibri_cli, decibri, audio_backend, target, rust_version}`. An `insta` snapshot test pins it. Do not add, remove, or rename fields without explicit approval.
- **Other JSON outputs** (`devices --json`, etc.) are explicitly marked unstable until v1.0.0. Changes are allowed but should be documented in CHANGELOG.md.

## CI

Required checks on every PR (see `.github/workflows/ci.yml`):

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo deny check`
- `cargo audit`
- Build + test on Windows, macOS, Linux

Hardware tests (real microphone and speaker) are local-only and are NOT run in CI. Headless runners do not have audio devices.

## npm wrapper

The npm wrapper lives at `npm/decibri-cli/`. `install.js` is JavaScript (not Rust). Changes to the wrapper require manual cross-platform install testing on Windows, macOS, and Linux before merging. Simulate network failures (blocked DNS) and verify the error message points at the manual-download fallback.

## Changelog

- Update `CHANGELOG.md` when adding features, fixing bugs, or making breaking changes.
- Use [Keep a Changelog](https://keepachangelog.com) format. One bullet per change, concise.
- Add entries under the current unreleased section in the appropriate subsection (Added, Changed, Fixed, Removed).
