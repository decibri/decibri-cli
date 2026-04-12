'use strict';

const { test } = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');
const { readVersion } = require('../install.js');

test('readVersion returns a semver-like string', () => {
    const v = readVersion();
    assert.strictEqual(typeof v, 'string');
    assert.match(v, /^\d+\.\d+\.\d+/);
});

test('readVersion matches package.json version field', () => {
    const pkgPath = path.join(__dirname, '..', 'package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    assert.strictEqual(readVersion(), pkg.version);
});

test('package.json is at placeholder 0.0.1 during development', () => {
    // Guard: Phase 6 ships the wrapper at the reserved placeholder version.
    // Phase 7.5 bumps it to 0.1.0-alpha.1 for the real alpha publish.
    // If this assertion fires, someone bumped the version unintentionally.
    const pkgPath = path.join(__dirname, '..', 'package.json');
    const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
    assert.strictEqual(
        pkg.version,
        '0.0.1',
        'npm package version should stay at 0.0.1 until Phase 7.5 bumps it for alpha',
    );
});
