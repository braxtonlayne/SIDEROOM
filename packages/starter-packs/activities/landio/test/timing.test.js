// @ts-check
// The timing helper (test/support/timing.js) is load-proof and still catches a real regression.
import assert from 'node:assert/strict';
import { median, modeOverhead } from './support/timing.js';

/** A fake clock: the base costs 1 ms a tick, the mode `extra` more; `spikes` batches (by call number) cost 5 ms more. */
function fake(extra, spikes = new Set()) {
  let call = 0;
  return (/** @type {string} */ m) => {
    const n = call++;
    return 1 + (m === 'mode' ? extra : 0) + (spikes.has(n) ? 5 : 0);
  };
}

test('median', () => {
  assert.equal(median([3, 1, 2]), 2);
  assert.equal(median([4, 1, 3, 2]), 2.5);
});

test('two load spikes on the mode under test do not fail a cheap mode', () => {
  // Calls 0 and 1 warm up; then pairs. Spike two of the seven mode batches.
  const r = modeOverhead('mode', 'base', { measure: fake(0.05, new Set([3, 8])) });
  assert.ok(r.extra < 0.3, String(r.extra));
  assert.equal(r.diffs.filter((d) => d > 1).length, 2, 'the spikes were there');
});

test('a mode that really costs 0.4 ms more per tick still fails, even with a spike on the base', () => {
  const r = modeOverhead('mode', 'base', { measure: fake(0.4, new Set([2, 5])) });
  assert.ok(r.extra >= 0.3, String(r.extra));
});
