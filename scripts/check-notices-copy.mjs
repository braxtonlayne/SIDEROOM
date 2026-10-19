#!/usr/bin/env node
// M-01 (D-287): apps/web/public/THIRD_PARTY_NOTICES.md is a build-time copy of the repo-root file, not a second
// source. This fails when the two differ; `--write` refreshes the copy.
import { copyFileSync, existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const source = join(root, 'THIRD_PARTY_NOTICES.md');
const copy = join(root, 'apps', 'web', 'public', 'THIRD_PARTY_NOTICES.md');

if (process.argv.includes('--write')) {
  copyFileSync(source, copy);
  console.log('Copied THIRD_PARTY_NOTICES.md into apps/web/public/.');
  process.exit(0);
}
if (!existsSync(copy) || !readFileSync(source).equals(readFileSync(copy))) {
  console.error(
    'apps/web/public/THIRD_PARTY_NOTICES.md differs from THIRD_PARTY_NOTICES.md. Run: node scripts/check-notices-copy.mjs --write',
  );
  process.exit(1);
}
console.log('THIRD_PARTY_NOTICES.md copy is identical.');
