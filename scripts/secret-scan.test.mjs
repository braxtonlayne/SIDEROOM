// RDY-SEC-005: the secret scanner catches each pattern, ignores placeholders, honours the allow-list,
// never prints a value, and is clean on the current tree. Secret-shaped strings are assembled at run time
// so this file does not trip the scanner itself.
import test from 'node:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import {
  applyAllowList,
  isStrayEnvFile,
  loadAllowList,
  scanAdded,
  scanText,
} from './secret-scan.mjs';

const rand = (n, alphabet = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789') =>
  Array.from({ length: n }, (_, i) => alphabet[(i * 7 + 3) % alphabet.length]).join('');

const names = (text) => scanText('x.txt', text).map((f) => f.pattern);

test('private key block', () => {
  assert.deepEqual(names(`-----BEGIN ${'RSA '}PRIVATE KEY-----`), ['private-key-block']);
  assert.deepEqual(names(`-----BEGIN ${'OPENSSH '}PRIVATE KEY-----`), ['private-key-block']);
  assert.deepEqual(names('-----BEGIN PUBLIC KEY-----'), []);
});

test('aws access key id', () => {
  assert.deepEqual(names(`id = ${'AKIA'}${rand(16, 'ABCDEFGHIJKLMNOP0123456789')}`), [
    'aws-access-key-id',
  ]);
  assert.deepEqual(names('no key here AKIA short'), []);
});

test('github token shapes', () => {
  assert.deepEqual(names(`t=${'ghp_'}${rand(36)}`), ['github-token']);
  assert.deepEqual(names(`t=${'github_pat_'}${rand(60)}`), ['github-token']);
  assert.deepEqual(names(`t=${'ghp_'}short`), []);
});

test('discord token shape', () => {
  const tok = `${'MTA'}${rand(21)}.${rand(6)}.${rand(30)}`;
  assert.deepEqual(names(`token: ${tok}`), ['discord-token']);
  assert.deepEqual(names('a.b.c'), []);
});

test('SR_ secret assigned a literal value', () => {
  assert.deepEqual(names(`SR_SERVER__SECRET_KEY=${rand(40)}`), ['sr-secret-assignment']);
  assert.deepEqual(names(`  SR_SMTP__PASSWORD: "${rand(20)}"`), ['sr-secret-assignment']);
});

test('SR_ secret placeholders and computed values are not findings', () => {
  assert.deepEqual(names('SR_SERVER__SECRET_KEY='), []);
  assert.deepEqual(names('SR_SERVER__SECRET_KEY=<64 hex chars>'), []);
  assert.deepEqual(names('SR_SERVER__SECRET_KEY=${SR_SECRET_KEY}'), []);
  assert.deepEqual(names('SR_SERVER__SECRET_KEY="$(openssl rand -hex 32)"'), []);
  assert.deepEqual(names('SR_SERVER__SECRET_KEY=change-me-please'), []);
  assert.deepEqual(names('SR_PAYMENTS_ALLOW_PRIVATE_HOSTS=1'), []);
});

test('password in a URL', () => {
  assert.deepEqual(names(`postgres://app:${rand(16)}@db.internal/sideroom`), ['password-in-url']);
  assert.deepEqual(names('postgres://app:${DB_PASSWORD}@db/sideroom'), []);
  assert.deepEqual(names('postgres://sideroom:sideroom@localhost/x'), []);
  assert.deepEqual(names(`https://host/path?x=1&password=${rand(12)}`), ['password-query-param']);
  assert.deepEqual(names('https://host/path?password=<pw>'), []);
});

test('stray .env files are findings, examples are not', () => {
  assert.equal(isStrayEnvFile('.env'), true);
  assert.equal(isStrayEnvFile('deploy/compose/.env.prod'), true);
  assert.equal(isStrayEnvFile('deploy/compose/.env.prod.example'), false);
  assert.equal(isStrayEnvFile('apps/web/.env.local'), true);
  assert.equal(isStrayEnvFile('docs/environment.md'), false);
  assert.deepEqual(scanAdded('deploy/.env', []), [
    { file: 'deploy/.env', line: 1, pattern: 'env-file' },
  ]);
});

test('added-line scanning reports the new line number', () => {
  const f = scanAdded('a.rs', [
    { line: 40, text: 'fine' },
    { line: 41, text: `-----BEGIN ${'EC '}PRIVATE KEY-----` },
  ]);
  assert.deepEqual(f, [{ file: 'a.rs', line: 41, pattern: 'private-key-block' }]);
});

test('findings never carry the matched value', () => {
  const secret = rand(36);
  const f = scanText('a.txt', `t=${'ghp_'}${secret}`);
  assert.equal(JSON.stringify(f).includes(secret), false);
  assert.deepEqual(Object.keys(f[0]).sort(), ['file', 'line', 'pattern']);
});

test('allow-list filters by path glob and pattern', () => {
  const found = [
    { file: 'a/b.yml', line: 1, pattern: 'password-in-url' },
    { file: 'a/b.yml', line: 2, pattern: 'github-token' },
    { file: 'c/d.yml', line: 3, pattern: 'password-in-url' },
  ];
  const left = applyAllowList(found, [
    { path: 'a/*.yml', pattern: 'password-in-url', reason: 'x' },
  ]);
  assert.deepEqual(
    left.map((f) => `${f.file}:${f.line}`),
    ['a/b.yml:2', 'c/d.yml:3'],
  );
  assert.equal(applyAllowList(found, [{ path: '**', reason: 'all' }]).length, 0);
});

test('every allow-list entry has a reason', () => {
  for (const a of loadAllowList()) {
    assert.ok(a.path && a.reason && a.reason.length > 10, JSON.stringify(a));
  }
});

test('the current tree is clean', () => {
  const script = join(dirname(fileURLToPath(import.meta.url)), 'secret-scan.mjs');
  const r = spawnSync(process.execPath, [script], { encoding: 'utf8' });
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stdout, /clean/);
});
