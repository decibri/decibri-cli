'use strict';

// Postinstall script for decibri-cli.
//
// Runs once when `npm install -g decibri-cli` (or local install) completes.
// Downloads the platform-appropriate binary from the GitHub Release for this
// package's version, verifies its SHA256, extracts it, and places it next
// to the JS shim at bin/decibri-bin (or .exe on Windows).
//
// Zero npm dependencies: this script runs before any deps are available,
// so everything uses Node stdlib (https, crypto, fs, child_process).

const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const https = require('node:https');
const { execSync } = require('node:child_process');
const { URL } = require('node:url');

const RELEASE_BASE = 'https://github.com/decibri/decibri-cli/releases/download';
const ISSUES_URL = 'https://github.com/decibri/decibri-cli/issues';
const MAX_REDIRECTS = 5;
const REQUEST_TIMEOUT_MS = 30_000;
// Initial attempt + 3 retries with exponential backoff. Worst case wait:
// 4 * 30s timeout + 1s + 2s + 4s = 127s. Long but bounded; users on broken
// networks see a clear error and manual-install instructions.
const RETRY_DELAYS_MS = [1_000, 2_000, 4_000];

const BIN_DIR = path.join(__dirname, 'bin');

function readVersion() {
    const pkgPath = path.join(__dirname, 'package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    return pkg.version;
}

// detectPlatformFor is the testable core; detectPlatform() calls it with the
// current process values. Split so unit tests can cover every platform/arch
// combo without mutating process.platform (which is read-only).
function detectPlatformFor(platform, arch) {
    if (platform === 'win32' && arch === 'x64') {
        return {
            archive: 'decibri-x86_64-pc-windows-msvc.zip',
            binaryName: 'decibri.exe',
            isZip: true,
        };
    }
    if (platform === 'linux' && arch === 'x64') {
        return {
            archive: 'decibri-x86_64-unknown-linux-gnu.tar.gz',
            binaryName: 'decibri',
            isZip: false,
        };
    }
    if (platform === 'linux' && arch === 'arm64') {
        return {
            archive: 'decibri-aarch64-unknown-linux-gnu.tar.gz',
            binaryName: 'decibri',
            isZip: false,
        };
    }
    if (platform === 'darwin') {
        // Universal binary covers both x86_64 and aarch64 Macs. Slightly
        // larger download but simpler: no arch detection races, no risk of
        // shipping the wrong slice to rosetta users.
        return {
            archive: 'decibri-universal2-apple-darwin.tar.gz',
            binaryName: 'decibri',
            isZip: false,
        };
    }
    return null;
}

function detectPlatform() {
    return detectPlatformFor(process.platform, process.arch);
}

function getUnsupportedMessage(platform, arch) {
    return [
        'decibri-cli does not support this platform.',
        `Platform: ${platform}, Architecture: ${arch}`,
        'Supported platforms: win32 x64, linux x64, linux arm64, darwin (universal).',
        `Please file an issue at ${ISSUES_URL}`,
    ].join('\n');
}

// Parse the GNU `sha256sum` output format: "<64 hex chars>  <filename>" with
// exactly two spaces between. Accepts CRLF line endings and blank lines.
function parseChecksums(text, filename) {
    const lines = text.split(/\r?\n/);
    for (const raw of lines) {
        const line = raw.trim();
        if (!line) continue;
        const match = line.match(/^([a-f0-9]{64})\s+(.+)$/i);
        if (match && match[2].trim() === filename) {
            return match[1].toLowerCase();
        }
    }
    return null;
}

function sha256(buffer) {
    return crypto.createHash('sha256').update(buffer).digest('hex');
}

function sleep(ms) {
    return new Promise((resolve) => setTimeout(resolve, ms));
}

// Download a URL into an in-memory Buffer, following redirects. Rejects on
// non-HTTPS redirects (defense against downgrade attacks), on >5 hops, on
// any non-200 response, and on timeout.
function downloadBuffer(urlString, options = {}) {
    const {
        maxRedirects = MAX_REDIRECTS,
        timeoutMs = REQUEST_TIMEOUT_MS,
    } = options;

    return new Promise((resolve, reject) => {
        const attempt = (currentUrl, redirectsRemaining) => {
            let parsed;
            try {
                parsed = new URL(currentUrl);
            } catch (err) {
                return reject(new Error(`invalid URL: ${currentUrl}`));
            }
            if (parsed.protocol !== 'https:') {
                return reject(
                    new Error(`refusing to follow non-HTTPS URL: ${currentUrl}`),
                );
            }

            const req = https.get(parsed, { timeout: timeoutMs }, (res) => {
                const status = res.statusCode ?? 0;
                if ([301, 302, 303, 307, 308].includes(status)) {
                    res.resume();
                    if (redirectsRemaining <= 0) {
                        return reject(new Error('too many redirects'));
                    }
                    const loc = res.headers.location;
                    if (!loc) {
                        return reject(new Error(`redirect ${status} with no Location header`));
                    }
                    const next = new URL(loc, currentUrl).toString();
                    return attempt(next, redirectsRemaining - 1);
                }
                if (status !== 200) {
                    res.resume();
                    return reject(
                        new Error(`HTTP ${status} ${res.statusMessage || ''} at ${currentUrl}`),
                    );
                }
                const chunks = [];
                res.on('data', (chunk) => chunks.push(chunk));
                res.on('end', () => resolve(Buffer.concat(chunks)));
                res.on('error', reject);
            });
            req.on('timeout', () => {
                req.destroy(new Error(`request timed out after ${timeoutMs}ms`));
            });
            req.on('error', reject);
        };
        attempt(urlString, maxRedirects);
    });
}

async function downloadWithRetry(url, label) {
    let lastError;
    const maxAttempts = RETRY_DELAYS_MS.length + 1;
    for (let attempt = 0; attempt < maxAttempts; attempt++) {
        try {
            return await downloadBuffer(url);
        } catch (err) {
            lastError = err;
            if (attempt < RETRY_DELAYS_MS.length) {
                const delay = RETRY_DELAYS_MS[attempt];
                console.error(
                    `[decibri-cli install] ${label} attempt ${attempt + 1} failed: ` +
                        `${err.message}. Retrying in ${delay}ms...`,
                );
                await sleep(delay);
            }
        }
    }
    throw lastError;
}

function extractArchive(archivePath, destDir, isZip) {
    if (isZip) {
        // Escape single quotes for PowerShell single-quoted strings by
        // doubling them. Rely on system PowerShell (present on every
        // supported Windows version since Win7). -NoProfile keeps user
        // profile scripts out of the way.
        const escPath = archivePath.replace(/'/g, "''");
        const escDest = destDir.replace(/'/g, "''");
        const ps = `Expand-Archive -Path '${escPath}' -DestinationPath '${escDest}' -Force`;
        execSync(`powershell -NoProfile -NonInteractive -Command "${ps}"`, {
            stdio: 'inherit',
        });
    } else {
        // POSIX tar accepts forward-slashed paths; no quoting gymnastics
        // beyond standard double-quote escaping.
        execSync(`tar -xzf "${archivePath}" -C "${destDir}"`, { stdio: 'inherit' });
    }
}

async function main() {
    const version = readVersion();
    const info = detectPlatform();

    if (!info) {
        console.error(getUnsupportedMessage(process.platform, process.arch));
        process.exit(1);
    }

    const { archive, binaryName, isZip } = info;
    const archiveUrl = `${RELEASE_BASE}/v${version}/${archive}`;
    const checksumsUrl = `${RELEASE_BASE}/v${version}/SHA256SUMS`;

    console.error(
        `[decibri-cli] installing v${version} for ${process.platform}-${process.arch}`,
    );
    console.error(`[decibri-cli] archive: ${archive}`);

    // 1. Download SHA256SUMS first. If the release doesn't exist, this is
    //    where we find out, before pulling down a multi-MB tarball.
    let checksumsText;
    try {
        const buf = await downloadWithRetry(checksumsUrl, 'SHA256SUMS');
        checksumsText = buf.toString('utf8');
    } catch (err) {
        console.error(`\nFailed to download SHA256SUMS from ${checksumsUrl}`);
        console.error(`Error: ${err.message}\n`);
        console.error('Possible causes:');
        console.error(`  - Release v${version} does not exist yet`);
        console.error('  - No internet connection');
        console.error('  - Corporate firewall blocking github.com');
        console.error('  - GitHub Releases temporarily unavailable\n');
        console.error('Manual install:');
        console.error(`  1. Download ${archiveUrl}`);
        console.error('  2. Extract and place the binary on your PATH');
        process.exit(1);
    }

    const expectedHash = parseChecksums(checksumsText, archive);
    if (!expectedHash) {
        console.error(`\nNo SHA256 entry for ${archive} in SHA256SUMS.`);
        console.error(`Release v${version} may be corrupted or incomplete.`);
        console.error(`Please file an issue at ${ISSUES_URL}`);
        process.exit(1);
    }

    // 2. Download the archive.
    let archiveBuf;
    try {
        archiveBuf = await downloadWithRetry(archiveUrl, `archive ${archive}`);
    } catch (err) {
        console.error(`\nFailed to download ${archive}`);
        console.error(`Error: ${err.message}\n`);
        console.error('Manual install:');
        console.error(`  1. Download ${archiveUrl}`);
        console.error(`  2. Verify against ${checksumsUrl}`);
        console.error('  3. Extract and place the binary on your PATH');
        process.exit(1);
    }

    // 3. Verify SHA256. On mismatch, delete and redownload once before
    //    failing; this covers transient CDN corruption without silently
    //    accepting mismatched binaries.
    let actualHash = sha256(archiveBuf);
    if (actualHash !== expectedHash) {
        console.error(
            `[decibri-cli install] SHA256 mismatch on first download, retrying...`,
        );
        console.error(`  expected: ${expectedHash}`);
        console.error(`  got:      ${actualHash}`);
        archiveBuf = await downloadWithRetry(archiveUrl, `${archive} (verify retry)`);
        actualHash = sha256(archiveBuf);
        if (actualHash !== expectedHash) {
            console.error(`\nSHA256 mismatch on retry:`);
            console.error(`  expected: ${expectedHash}`);
            console.error(`  got:      ${actualHash}\n`);
            console.error('This may indicate a corrupted download, a mirror');
            console.error('hijack, or a publishing error. Aborting install.');
            console.error(`Please file an issue at ${ISSUES_URL}`);
            process.exit(1);
        }
    }

    // 4. Write to disk, extract, move the real binary next to the shim.
    fs.mkdirSync(BIN_DIR, { recursive: true });
    const archivePath = path.join(BIN_DIR, archive);
    fs.writeFileSync(archivePath, archiveBuf);

    const tmpExtractDir = path.join(BIN_DIR, '__extract');
    fs.rmSync(tmpExtractDir, { recursive: true, force: true });
    fs.mkdirSync(tmpExtractDir, { recursive: true });

    try {
        extractArchive(archivePath, tmpExtractDir, isZip);
    } catch (err) {
        console.error(`\nFailed to extract ${archive}: ${err.message}`);
        console.error('Make sure `tar` (Unix) or PowerShell (Windows) is available.');
        process.exit(1);
    }

    const extractedBinary = path.join(tmpExtractDir, binaryName);
    if (!fs.existsSync(extractedBinary)) {
        console.error(
            `\nExpected binary not found after extraction: ${extractedBinary}`,
        );
        console.error('Archive contents may have changed upstream.');
        console.error(`Please file an issue at ${ISSUES_URL}`);
        process.exit(1);
    }

    const destName = process.platform === 'win32' ? 'decibri-bin.exe' : 'decibri-bin';
    const destPath = path.join(BIN_DIR, destName);
    // Remove any stale binary from a previous install.
    fs.rmSync(destPath, { force: true });
    fs.renameSync(extractedBinary, destPath);

    if (process.platform !== 'win32') {
        fs.chmodSync(destPath, 0o755);
    }

    // 5. Cleanup temporary files. Keep nothing around for debugging: if
    //    reinstall is needed, re-download is cheap.
    fs.rmSync(archivePath, { force: true });
    fs.rmSync(tmpExtractDir, { recursive: true, force: true });

    const sizeKb = (archiveBuf.length / 1024).toFixed(0);
    console.error(
        `[decibri-cli] installed ${destName} (${sizeKb} KB archive, sha256 verified)`,
    );
}

// Exports for unit tests. The main() entry point below is gated on
// require.main === module so `require('./install.js')` in tests returns the
// pure functions without triggering a real install.
module.exports = {
    detectPlatform,
    detectPlatformFor,
    getUnsupportedMessage,
    parseChecksums,
    readVersion,
    sha256,
};

if (require.main === module) {
    main().catch((err) => {
        console.error(
            `[decibri-cli install] unexpected error: ${err && err.stack ? err.stack : err}`,
        );
        process.exit(1);
    });
}
