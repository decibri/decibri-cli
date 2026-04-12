'use strict';

const { test } = require('node:test');
const assert = require('node:assert');
const { detectPlatformFor, getUnsupportedMessage } = require('../install.js');

test('win32 x64 maps to windows msvc zip', () => {
    const r = detectPlatformFor('win32', 'x64');
    assert.deepStrictEqual(r, {
        archive: 'decibri-x86_64-pc-windows-msvc.zip',
        binaryName: 'decibri.exe',
        isZip: true,
    });
});

test('linux x64 maps to linux gnu tar.gz', () => {
    const r = detectPlatformFor('linux', 'x64');
    assert.deepStrictEqual(r, {
        archive: 'decibri-x86_64-unknown-linux-gnu.tar.gz',
        binaryName: 'decibri',
        isZip: false,
    });
});

test('linux arm64 maps to aarch64 linux gnu tar.gz', () => {
    const r = detectPlatformFor('linux', 'arm64');
    assert.deepStrictEqual(r, {
        archive: 'decibri-aarch64-unknown-linux-gnu.tar.gz',
        binaryName: 'decibri',
        isZip: false,
    });
});

test('darwin x64 maps to universal2', () => {
    const r = detectPlatformFor('darwin', 'x64');
    assert.strictEqual(r.archive, 'decibri-universal2-apple-darwin.tar.gz');
    assert.strictEqual(r.isZip, false);
});

test('darwin arm64 also maps to universal2 (no arch split)', () => {
    const r = detectPlatformFor('darwin', 'arm64');
    assert.strictEqual(r.archive, 'decibri-universal2-apple-darwin.tar.gz');
    assert.strictEqual(r.isZip, false);
});

test('unsupported OS returns null', () => {
    assert.strictEqual(detectPlatformFor('freebsd', 'x64'), null);
    assert.strictEqual(detectPlatformFor('openbsd', 'x64'), null);
    assert.strictEqual(detectPlatformFor('aix', 'ppc64'), null);
});

test('unsupported arch returns null', () => {
    assert.strictEqual(detectPlatformFor('linux', 'mips64'), null);
    assert.strictEqual(detectPlatformFor('linux', 's390x'), null);
    assert.strictEqual(detectPlatformFor('win32', 'arm64'), null);
    assert.strictEqual(detectPlatformFor('win32', 'ia32'), null);
});

test('getUnsupportedMessage mentions platform, arch, and issues URL', () => {
    const msg = getUnsupportedMessage('linux', 'mips64');
    assert.ok(msg.includes('linux'), 'should mention platform');
    assert.ok(msg.includes('mips64'), 'should mention arch');
    assert.ok(msg.includes('github.com/decibri/decibri-cli/issues'), 'should link to issues');
    assert.ok(msg.includes('Supported platforms'), 'should list supported platforms');
});
