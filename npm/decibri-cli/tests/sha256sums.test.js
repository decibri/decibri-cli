'use strict';

const { test } = require('node:test');
const assert = require('node:assert');
const { parseChecksums, sha256 } = require('../install.js');

// Sample SHA256SUMS file in the GNU sha256sum format: "<hex>  <filename>"
// (two spaces). The real release pipeline emits this shape; see
// build-release.yml's `Generate SHA256SUMS` step.
const SAMPLE = [
    'abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd  decibri-x86_64-unknown-linux-gnu.tar.gz',
    'def456def456def456def456def456def456def456def456def456def456defa  decibri-x86_64-pc-windows-msvc.zip',
    '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  decibri-aarch64-unknown-linux-gnu.tar.gz',
    'fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210  decibri-universal2-apple-darwin.tar.gz',
].join('\n');

test('parses GNU sha256sum format (two spaces)', () => {
    assert.strictEqual(
        parseChecksums(SAMPLE, 'decibri-x86_64-unknown-linux-gnu.tar.gz'),
        'abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd',
    );
});

test('finds entries for all four platform archives', () => {
    assert.ok(parseChecksums(SAMPLE, 'decibri-x86_64-pc-windows-msvc.zip'));
    assert.ok(parseChecksums(SAMPLE, 'decibri-aarch64-unknown-linux-gnu.tar.gz'));
    assert.ok(parseChecksums(SAMPLE, 'decibri-universal2-apple-darwin.tar.gz'));
});

test('returns null for missing filename', () => {
    assert.strictEqual(parseChecksums(SAMPLE, 'nonexistent.tar.gz'), null);
});

test('handles CRLF line endings', () => {
    const crlf = SAMPLE.replace(/\n/g, '\r\n');
    assert.strictEqual(
        parseChecksums(crlf, 'decibri-x86_64-pc-windows-msvc.zip'),
        'def456def456def456def456def456def456def456def456def456def456defa',
    );
});

test('ignores blank lines at start, middle, and end', () => {
    const padded = '\n\n' + SAMPLE + '\n\n\n';
    assert.strictEqual(
        parseChecksums(padded, 'decibri-universal2-apple-darwin.tar.gz'),
        'fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210',
    );
});

test('hash is returned lowercase even if input is uppercase', () => {
    const upper = 'ABC123ABC123ABC123ABC123ABC123ABC123ABC123ABC123ABC123ABC123ABCD  foo.tar.gz';
    assert.strictEqual(parseChecksums(upper, 'foo.tar.gz'), 'abc123abc123abc123abc123abc123abc123abc123abc123abc123abc123abcd');
});

test('rejects short (non-64-char) hashes', () => {
    const bad = 'abc  foo.tar.gz';
    assert.strictEqual(parseChecksums(bad, 'foo.tar.gz'), null);
});

test('sha256 of empty buffer matches known value', () => {
    // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    assert.strictEqual(
        sha256(Buffer.alloc(0)),
        'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    );
});

test('sha256 of ASCII "abc" matches RFC 6234 test vector', () => {
    assert.strictEqual(
        sha256(Buffer.from('abc', 'utf8')),
        'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad',
    );
});
