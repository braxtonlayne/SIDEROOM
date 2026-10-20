#!/usr/bin/env node
// Production-dependency licence gate (K-13, M-08, D-297, D-309).
// Usage: pnpm licenses list --json --prod | node scripts/check-licenses.mjs
// Fails on any licence outside the allow-list. The two copyleft packages are allowed by name only, so a second
// copyleft package cannot arrive unnoticed. Their notices live in THIRD_PARTY_NOTICES.md.
import { readFileSync } from 'node:fs';

const ALLOWED = new Set([
  'MIT',
  'ISC',
  'BSD-2-Clause',
  'BSD-3-Clause',
  'Apache-2.0',
  '0BSD',
  'CC0-1.0',
  'Unlicense',
  'MPL-2.0',
]);

// licence expression -> the only packages allowed to carry it
const BY_NAME = new Map([
  // JASSUB bundles libass (ISC), FreeType (FTL), fribidi (LGPL-2.1), etc.; own chunk, worker and wasm shipped as
  // separate unminified files (LGPL-2.1 section 6). Accepted by name because D-314 reads this package.json
  // expression as the licence that governs (stricter than the MIT LICENSE file, which covers the JS wrapper only);
  // the texts are in THIRD_PARTY_NOTICES.md.
  [
    'LGPL-2.1-or-later AND (FTL OR GPL-2.0-or-later) AND MIT AND MIT-Modern-Variant AND ISC AND NTP AND Zlib AND BSL-1.0',
    new Set(['jassub']),
  ],
  // Hard dependency of jassub, shipped as is; GPL is acceptable to the owner (D-309).
  ['GPL-3.0', new Set(['rvfc-polyfill'])],
]);

function violations(report) {
  const bad = [];
  for (const [licence, pkgs] of Object.entries(report)) {
    if (ALLOWED.has(licence)) continue;
    const names = BY_NAME.get(licence);
    for (const p of pkgs) if (!names?.has(p.name)) bad.push(`${p.name}: ${licence}`);
  }
  return bad;
}

const bad = violations(JSON.parse(readFileSync(0, 'utf8')));
if (bad.length) {
  console.error(`Licence check failed:\n  ${bad.join('\n  ')}`);
  process.exit(1);
}
console.log('Licence check ok.');
