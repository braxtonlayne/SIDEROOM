// SEC-AUD-235: i18n-audit.mjs --max <n> fails the run when the hard-coded string total is over n.
// The script walks the disk when it is loaded, so each case runs it as a child process on a small fixture folder.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = fileURLToPath(new URL('./i18n-audit.mjs', import.meta.url));
const run = (...args) => spawnSync(process.execPath, [SCRIPT, ...args], { encoding: 'utf8' });

// Two hard-coded strings in one file (a JSX text and an aria-label), one extracted string in another.
const root = mkdtempSync(join(tmpdir(), 'i18n-audit-'));
mkdirSync(join(root, 'area'));
writeFileSync(
  join(root, 'area', 'Panel.tsx'),
  `export const Panel = () => (
  <div aria-label="Close the panel">
    <p>Nothing here yet</p>
  </div>
);
`,
);
writeFileSync(
  join(root, 'area', 'Clean.tsx'),
  `export const Clean = () => <p>{t('clean.title')}</p>;\n`,
);
test.after(() => rmSync(root, { recursive: true, force: true }));

test('without --max the script reports the total and exits 0', () => {
  const r = run(root);
  assert.equal(r.status, 0);
  assert.match(r.stdout, /TOTAL 2/);
});

test('--max under the total exits 1 and says how to fix it', () => {
  const r = run(root, '--max', '1');
  assert.equal(r.status, 1);
  assert.match(r.stdout, /Panel\.tsx/);
  assert.match(r.stderr, /2 hard-coded strings, over the limit of 1/);
  assert.match(r.stderr, /message key: t\('\.\.\.'\)/);
});

test('--max at or over the total exits 0, and its place on the command line does not matter', () => {
  assert.equal(run(root, '--max', '2').status, 0);
  assert.equal(run('--max', '5', root).status, 0);
});

test('--max 0 on a clean folder passes', () => {
  const clean = mkdtempSync(join(tmpdir(), 'i18n-audit-clean-'));
  try {
    writeFileSync(join(clean, 'Ok.tsx'), `export const Ok = () => <p>{t('ok')}</p>;\n`);
    assert.equal(run(clean, '--max', '0').status, 0);
  } finally {
    rmSync(clean, { recursive: true, force: true });
  }
});

test('a missing or bad --max value is a usage error (exit 2)', () => {
  for (const args of [
    [root, '--max'],
    [root, '--max', 'x'],
    [root, '--max', '-1'],
    [root, '--max', '1.5'],
  ]) {
    const r = run(...args);
    assert.equal(r.status, 2, args.join(' '));
    assert.match(r.stderr, /--max needs a non-negative integer/);
    assert.match(r.stderr, /usage: node scripts\/i18n-audit\.mjs/);
  }
});

test('--help prints the usage line and exits 0', () => {
  const r = run('--help');
  assert.equal(r.status, 0);
  assert.match(r.stdout, /usage: node scripts\/i18n-audit\.mjs/);
});
