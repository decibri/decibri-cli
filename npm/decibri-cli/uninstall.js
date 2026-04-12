'use strict';

// Preuninstall script. Removes the downloaded binary so `npm uninstall -g`
// leaves no orphaned files. npm itself handles the rest of the package
// directory; this script only cleans up files install.js created outside
// the npm-managed set.

const fs = require('node:fs');
const path = require('node:path');

const BIN_DIR = path.join(__dirname, 'bin');

const targets = [
    path.join(BIN_DIR, 'decibri-bin'),
    path.join(BIN_DIR, 'decibri-bin.exe'),
];

for (const p of targets) {
    try {
        fs.rmSync(p, { force: true });
    } catch (_err) {
        // Best-effort cleanup. If we can't remove it, npm will either remove
        // it when it nukes the package directory, or the user will see it
        // as an orphan which they can delete manually.
    }
}
