// ACT-EMU-003: the by-name core rules and the lock/doc/notices consistency checks, fed good and broken inputs.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { coreProblems, parseLock, tableProblems } from './lib/emu-cores.mjs';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const read = (p) => readFileSync(join(root, p), 'utf8');
const lock = read('deploy/emu-cores.lock');
const doc = read('deploy/EMULATOR.md');
const notices = read('THIRD_PARTY_NOTICES.md');
const row = (dest, lic = 'GPL-2.0') =>
  `${dest}\thttps://x/${dest}\t${'a'.repeat(64)}\t10\t${lic}\n`;

test('the real lock, table and notices agree', () => {
  assert.deepEqual(tableProblems(lock, doc), []);
  assert.deepEqual(coreProblems(lock, notices), []);
  assert.ok(parseLock(lock).length >= 7);
});

test('a rejected or unknown core is refused', () => {
  for (const bad of [
    'snes9x/snes9x-wasm.data',
    'genesis_plus_gx/g-wasm.data',
    'picodrive/p-wasm.data',
    'fbneo/f-wasm.data',
    'mame2003/m-wasm.data',
  ]) {
    assert.match(coreProblems(row(bad), notices).join('\n'), /not an allowed emulator core/, bad);
  }
});

test('a licence that differs from the by-name entry is refused', () => {
  assert.match(
    coreProblems(row('mgba/mgba-wasm.data', 'GPL-2.0'), notices).join('\n'),
    /mgba is MPL-2.0/,
  );
});

test('a core without its notice is refused', () => {
  const stripped = notices.replace('## Emulator cores', '## Something else');
  assert.match(coreProblems(lock, stripped).join('\n'), /no "Emulator cores" section/);
  assert.match(
    coreProblems(row('fceumm/f.data'), '## Emulator cores\n').join('\n'),
    /no GPL-2.0 notice for FCEUmm/,
  );
});

test('a table that drifted from the lock is refused', () => {
  const firstDigest = parseLock(lock)[1].sha256;
  const drifted = doc.replace(firstDigest, 'b'.repeat(64));
  const p = tableProblems(lock, drifted).join('\n');
  assert.match(p, /has no table row/);
  assert.match(p, /not in the lock/);
});
