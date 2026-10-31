// Run with: node --test scripts
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { checkTitle } from './check-pr-title.mjs';
import { linksIn, slug } from './check-doc-links.mjs';
import { classify, failedTests } from './flake-detect.mjs';
import { issueFor } from './inventory-to-issues.mjs';
import { parseInventoryText } from './lib/inventory.mjs';
import { staticImports } from './lib/bundle.mjs';
import { announceIps, announceValue, defaultRouteAddress, isLoopback } from './ice-announce.mjs';

test('PR titles need an inventory id or a conventional prefix', () => {
  assert.equal(checkTitle('feat(LNG-MSG-014): message edit'), null);
  assert.equal(checkTitle('XC-PWA-001, XC-PWA-002 service worker'), null);
  assert.equal(checkTitle('docs: fix a typo'), null);
  assert.equal(checkTitle('chore(deps): bump vite'), null);
  assert.match(checkTitle('Add a feature'), /no inventory ID/);
  assert.match(checkTitle('feat: something'), /no inventory ID/);
  assert.match(checkTitle('   '), /empty/);
});

test('inventory lines parse into items', () => {
  const items = parseInventoryText(
    '## XC-PWA\n- [x] **XC-PWA-001** [FE] [TEST] Manifest\n- [ ] **LNG-MSG-014** [BE] Edit\nnot an item\n',
    '18-cross-cutting.md',
  );
  assert.deepEqual(
    items.map((i) => [i.id, i.done, i.tags, i.title, i.section]),
    [
      ['XC-PWA-001', true, ['FE', 'TEST'], 'Manifest', 'XC-PWA'],
      ['LNG-MSG-014', false, ['BE'], 'Edit', 'XC-PWA'],
    ],
  );
  const issue = issueFor(items[1], { 18: 'All phases' });
  assert.deepEqual(issue.labels, ['area:cross-cutting', 'kind:BE']);
  assert.equal(issue.milestone, 'All phases');
});

test('doc links: relative links are found, code and fences are ignored, slugs match GitHub', () => {
  const md =
    'See [a](a.md) and [b](b.md#sec).\n`[no](ignored.md)`\n```\n[no](fenced.md)\n```\n[web](https://x.y)';
  assert.deepEqual(
    linksIn(md).map((l) => l.target),
    ['a.md', 'b.md#sec', 'https://x.y'],
  );
  assert.equal(slug('12 — Deployment & Operations'), '12--deployment--operations');
  assert.equal(slug('`sr-cli` backup'), 'sr-cli-backup');
});

test('flake detector separates flaky from broken tests', () => {
  const vitest =
    ' FAIL  src/a.test.ts > suite > one\n \u001b[31m FAIL \u001b[39m src/b.test.ts > two\n';
  assert.deepEqual(
    [...failedTests(vitest)],
    ['src/a.test.ts > suite > one', 'src/b.test.ts > two'],
  );
  assert.deepEqual(
    [...failedTests('test db::tests::lock ... FAILED\ntest ok ... ok')],
    ['db::tests::lock'],
  );
  // Playwright's list reporter (OPS-CI-001): the running index and the duration are not part of the name.
  assert.deepEqual(
    [
      ...failedTests(
        '  ✘  4 [chromium] › tests/a.spec.ts:9:1 › one (1.2s)\n  ✓  5 [chromium] › tests/a.spec.ts:20:1 › ok (2s)',
      ),
    ],
    ['[chromium] › tests/a.spec.ts:9:1 › one'],
  );
  const { flaky, broken } = classify([['x', 'y'], ['y'], ['y']]);
  assert.deepEqual(
    flaky.map((f) => f.name),
    ['x'],
  );
  assert.deepEqual(
    broken.map((f) => f.name),
    ['y'],
  );
});

test('bundle budget follows static imports only', () => {
  const code =
    'import{a as b}from"./vendor-1.js";import"./side-2.js";const x=()=>import("./lazy-3.js");export{y}from"./re-4.js";';
  assert.deepEqual(staticImports('assets/index-0.js', code), [
    'assets/vendor-1.js',
    'assets/side-2.js',
    'assets/re-4.js',
  ]);
});

test('ice-announce: the SFU announces a routable address so Firefox can pair with it', async () => {
  assert.equal(isLoopback('127.0.0.1'), true);
  assert.equal(isLoopback('127.1.2.3'), true);
  assert.equal(isLoopback('::1'), true);
  assert.equal(isLoopback('10.1.0.4'), false);
  // The figment env form (TOML array of strings) for SR_VOICE__ANNOUNCE_IPS.
  assert.equal(announceValue('10.1.0.4'), '["10.1.0.4"]');
  assert.deepEqual(await announceIps(async () => '10.1.0.4'), {
    value: '["10.1.0.4"]',
    routable: true,
  });
  // No route, or only loopback: keep the old default (Chromium still connects) and say so.
  for (const found of [null, '127.0.0.1', '0.0.0.0'])
    assert.deepEqual(await announceIps(async () => found), {
      value: '["127.0.0.1"]',
      routable: false,
    });
  // A socket that cannot connect (no default route) resolves null instead of throwing.
  const failing = () => {
    const handlers = {};
    return {
      once: (ev, fn) => (handlers[ev] = fn),
      connect: () => handlers.error(new Error('ENETUNREACH')),
      close() {},
    };
  };
  assert.equal(await defaultRouteAddress('192.0.2.1', failing), null);
  const answering = () => ({
    once() {},
    connect: (_port, _host, cb) => cb(),
    address: () => ({ address: '10.1.0.4' }),
    close() {},
  });
  assert.equal(await defaultRouteAddress('192.0.2.1', answering), '10.1.0.4');
});
