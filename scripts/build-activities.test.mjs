// ACT-A3D-001 (wave 46): every built-in Activity is registered, and a built manifest keeps the package manifest's
// capabilities and `connect` list (the server reviews and serves the zip's sideroom.json, so a lost `connect`
// would mean `connect-src 'none'` for arena3d's own model files).
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ACTIVITIES, checkTree } from './build-activities.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const ACT = join(ROOT, 'packages', 'starter-packs', 'activities');

const tree = (manifest) => ({
  'index.html': Buffer.from('<!doctype html>'),
  'sideroom.json': Buffer.from(JSON.stringify(manifest)),
});

test('every activity package with a sideroom.json is registered, arena3d included', () => {
  const packages = readdirSync(ACT, { withFileTypes: true })
    .filter((d) => d.isDirectory() && existsSync(join(ACT, d.name, 'sideroom.json')))
    .map((d) => d.name)
    .sort();
  assert.deepEqual(Object.keys(ACTIVITIES).sort(), packages);
  assert.ok(ACTIVITIES.arena3d, 'arena3d is registered');
});

test("arena3d's package manifest declares connect self and the capabilities the brief names", () => {
  const m = JSON.parse(readFileSync(join(ACT, 'arena3d', 'sideroom.json'), 'utf8'));
  assert.deepEqual(m.connect, ['self']);
  for (const c of ['datachannel', 'guest_play', 'lobby_channel', 'byos_read'])
    assert.ok(m.capabilities.includes(c), c);
  assert.equal(checkTree(tree(m), m).length, 0);
});

test('a built manifest that lost the connect list is refused', () => {
  const source = { capabilities: ['datachannel'], connect: ['self'] };
  const problems = checkTree(tree({ capabilities: ['datachannel'] }), source);
  assert.ok(
    problems.some((p) => p.includes('connect')),
    problems.join('; '),
  );
});

test('a built manifest that changed the capabilities is refused', () => {
  const source = { capabilities: ['datachannel', 'storage'] };
  const problems = checkTree(tree({ capabilities: ['datachannel'] }), source);
  assert.ok(
    problems.some((p) => p.includes('capabilities')),
    problems.join('; '),
  );
});

test('without a source manifest only the shape is checked (the old behaviour)', () => {
  assert.equal(checkTree(tree({ capabilities: ['datachannel'] })).length, 0);
  assert.ok(checkTree(tree({ capabilities: [] })).length > 0);
});
