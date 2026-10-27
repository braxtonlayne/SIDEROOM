// BYOS-UI-001: check-css-tokens.mjs flags raw colours and physical properties, and the real storage files pass.
import test from 'node:test';
import assert from 'node:assert/strict';
import { scan, targetFiles } from './check-css-tokens.mjs';
import { readFileSync } from 'node:fs';

const rules = (css, file = 'x.css') => scan(css, file).map((v) => v.rule);

test('tokens and logical properties pass', () => {
  assert.deepEqual(
    rules(
      '.a { color: var(--ui-text); margin-inline-start: 4px; border: 1px solid var(--ui-border); }',
    ),
    [],
  );
});

test('raw colours are flagged', () => {
  assert.deepEqual(rules('.a { color: #fff; }'), ['hex colour']);
  assert.deepEqual(rules('.a { background: rgb(0 0 0 / 50%); }'), ['colour function']);
  assert.deepEqual(rules('.a { color: oklch(0.5 0.1 200); }'), ['colour function']);
  assert.deepEqual(rules('.a { border: 1px solid red; }'), ['named colour "red"']);
  assert.deepEqual(rules('.a { color: var(--ui-text, red); }'), []);
});

test('physical sides are flagged', () => {
  assert.deepEqual(rules('.a { margin-left: 4px; }'), ['physical property (use the logical one)']);
  assert.deepEqual(rules('.a { border-top: 1px solid var(--ui-border); }'), [
    'physical property (use the logical one)',
  ]);
  assert.deepEqual(rules('.a { left: 0; }'), ['physical property (use the logical one)']);
});

test('inline style colours in TSX are flagged', () => {
  assert.deepEqual(rules('<div style={{ color: "#123456" }} />', 'x.tsx'), ['hex colour']);
  assert.deepEqual(rules('<div style={{ color: "var(--ui-text)" }} />', 'x.tsx'), []);
});

test('comments are ignored', () => {
  assert.deepEqual(rules('/* color: #fff; */ .a { color: var(--ui-text); }'), []);
});

test('the BYOS storage files pass', () => {
  const files = targetFiles();
  assert.ok(
    files.some((f) => f.endsWith('storage.css')),
    'storage.css is scanned',
  );
  for (const f of files) assert.deepEqual(scan(readFileSync(f, 'utf8'), f), [], f);
});
