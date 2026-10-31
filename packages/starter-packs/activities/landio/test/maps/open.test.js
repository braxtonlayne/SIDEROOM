// @ts-check
// Open Field: the shared map rules, and no walls or gold at all.
import assert from 'node:assert/strict';
import open from '../../shared/maps/open.js';
import { build, checkMap, share } from './helpers.js';

test('open: passes the shared map rules', () => {
  checkMap(open);
});

test('open: no walls and no gold', () => {
  const t = build(open, 100, 100, 'alpha');
  assert.equal(share(t, 0), 1);
});
