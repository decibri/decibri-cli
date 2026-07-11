# Security policy

## Supported versions

Only the latest published version of decibri-cli receives security updates.

| Version | Supported |
|---------|-----------|
| 0.2.x   | Yes       |
| < 0.2   | No        |

## Reporting a vulnerability

**Do not open a public issue for security vulnerabilities.**

Report security issues privately through GitHub's built-in vulnerability reporting flow:

👉 **<https://github.com/decibri/decibri-cli/security/advisories/new>**

This opens a private advisory visible only to you and the repository maintainers. GitHub handles the coordinated-disclosure workflow end to end: draft the advisory, discuss the fix in a private fork if needed, request a CVE, and publish when ready.

Include in your report:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix, if you have one
- Your preferred attribution (name and URL) or request for anonymity

### What to expect

- Acknowledgement within 7 days
- Initial assessment within 14 days
- A coordinated disclosure timeline agreed with the reporter
- Credit in the published advisory, unless you prefer anonymity

## Binary provenance

Every release binary is built via GitHub Actions and signed with a SLSA provenance attestation through Sigstore. The attestation covers the `decibri` binary itself, not the archive it ships in. Extract the archive, then verify the binary with the GitHub CLI:

```
tar xzf decibri-x86_64-unknown-linux-gnu.tar.gz
gh attestation verify decibri --owner decibri
```

The attestation proves the binary was produced by this repository's release workflow from a specific commit. A failed verification means either a corrupted download, a binary from a different source, or tampering.

## Checksum verification

Every release includes a `SHA256SUMS` file in the GNU sha256sum format. Verify your download before extracting:

```
# Linux / macOS
sha256sum -c SHA256SUMS --ignore-missing

# Windows PowerShell
Get-FileHash decibri-x86_64-pc-windows-msvc.zip -Algorithm SHA256
```

The `npm install -g decibri-cli` flow performs this verification automatically on every install. Manual downloads should verify before running.

## Security-relevant design choices

- **No network access at runtime.** `decibri-cli` does not connect to any remote service during capture, playback, or device enumeration. The only time the package touches the network is during `npm install` (downloading the binary from GitHub Releases) or `cargo install` (crates.io registry).
- **No file access outside user-specified paths.** The binary reads and writes only files named in command-line arguments.
- **No elevated permissions required.** The binary runs as the invoking user. It does not request, use, or need any elevated privileges.
- **SHA256-verified downloads in the npm wrapper.** Before extracting, the postinstall script verifies the downloaded archive against the release's `SHA256SUMS` manifest. A mismatch triggers one re-download; a second mismatch aborts the install with a clear error.
- **No telemetry, analytics, or phone-home.** The CLI does not collect or transmit any data.

## Known limitations

- **Unsigned Windows binaries.** Release binaries are not signed with an EV code-signing certificate. On first run, Windows SmartScreen may show a warning.
- **Unsigned macOS binaries.** Release binaries have no Apple Developer signing or notarization. macOS Gatekeeper warnings require `xattr -d com.apple.quarantine <path>` on direct downloads. The `npm install -g` path bypasses this.

Neither is a vulnerability; both are deferred cost decisions. Users who need signed binaries can build from source with `cargo install decibri-cli`.
