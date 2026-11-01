// OPS-CRASH-001 (wave 45-A): the secret-pattern grep of a built crash card bites on the hostile payload and passes
// the card the allow-list builds from it.
import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { findLeaks } from './check-crash-card.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const fixture = JSON.parse(
  readFileSync(join(ROOT, 'e2e', 'fixtures', 'crash-hostile.json'), 'utf8'),
);

test('every kind of secret in the hostile payload is caught by name', () => {
  const hits = findLeaks(JSON.stringify(fixture.payload, null, 2));
  for (const want of [
    'bearer-token',
    'jwt',
    'presigned-url-query',
    'sr-guest-token',
    'ipv4-address',
    'ipv6-address',
    'email-address',
    'url',
    'e2ee-key-material',
    'session-cookie',
    'aws-access-key-id',
  ]) {
    assert.ok(hits.includes(want), `${want} not caught: ${hits.join(', ')}`);
  }
});

test('the card the allow-list builds is clean', () => {
  assert.deepEqual(findLeaks(JSON.stringify(fixture.expect, null, 2)), []);
  assert.deepEqual(findLeaks(JSON.stringify(fixture.expect)), []);
});

test('a hit is reported by pattern name, never by value', () => {
  const hits = findLeaks('Authorization: Bearer abcdefghijklmnop1234');
  assert.deepEqual(hits, ['bearer-token']);
  assert.ok(!hits.join().includes('abcdefghijklmnop1234'));
});

test('ordinary frame names and commits are not flagged', () => {
  const card = JSON.stringify({
    commit: 'c0ea1dae0123456789abcdef0123456789abcdef',
    frames: [
      { file: 'assets/index-Bx3k9Qa1.js', line: 3, col: 9, fn: 'sr_e2ee_wasm::mls::decrypt' },
    ],
  });
  assert.deepEqual(findLeaks(card), []);
});

test('the self-test mode exits 0', () => {
  const out = execFileSync(
    process.execPath,
    [join(ROOT, 'scripts', 'check-crash-card.mjs'), '--self-test'],
    {
      encoding: 'utf8',
    },
  );
  assert.match(out, /expected card trips 0 patterns/);
});
