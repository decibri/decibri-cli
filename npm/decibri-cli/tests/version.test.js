'use strict';

const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const { readVersion } = require('../install.js');

test('readVersion returns a semver-like string', () => {
    const v = readVersion();
    assert.strictEqual(typeof v, 'string');
    // Accepts stable (0.1.0) and prerelease (0.1.0-alpha.1) forms.
    assert.match(v, /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/);
});

test('readVersion matches package.json version field', () => {
    const pkgPath = path.join(__dirname, '..', 'package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    assert.strictEqual(readVersion(), pkg.version);
});

// Drift-catcher: if someone bumps Cargo.toml without bumping package.json
// (or vice versa), this test fails loudly before the two can diverge in a
// published release. Cargo.toml lives three directories up from this file:
// tests/ -> decibri-cli/ -> npm/ -> repo root.
test('npm and cargo versions match', () => {
    const pkgPath = path.join(__dirname, '..', 'package.json');
    const cargoPath = path.join(__dirname, '..', '..', '..', 'Cargo.toml');

    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    const cargoToml = fs.readFileSync(cargoPath, 'utf8');

    // Match the [package] version (anchored to start of line; multiline
    // flag). The dep-table lines like `clap = { version = "4.5" }` are
    // indented and therefore don't match ^version at col 0.
    const match = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);
    assert.ok(match, 'Cargo.toml must have a [package] version field');
    const cargoVersion = match[1];

    assert.strictEqual(
        pkg.version,
        cargoVersion,
        `npm version (${pkg.version}) must match Cargo version (${cargoVersion})`,
    );
});
