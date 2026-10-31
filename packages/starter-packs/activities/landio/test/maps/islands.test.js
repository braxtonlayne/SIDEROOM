// @ts-check
// Islands: the shared map rules, the island grid, bridges between every neighbouring pair, channels that are wall.
import assert from 'node:assert/strict';
import islands, { layout } from '../../shared/maps/islands.js';
import { WALL, GOLD } from '../../shared/grid.js';
import { rand, seedFrom } from '../../shared/rng.js';
import { botTrial, build, checkMap, share } from './helpers.js';

const CASES = [
  { w: 40, h: 40, nx: 2, ny: 2 },
  { w: 60, h: 50, nx: 2, ny: 2 },
  { w: 100, h: 100, nx: 3, ny: 3 },
  { w: 200, h: 200, nx: 4, ny: 4 },
];

/** The layout the map used for this seed (same stream as the helpers). @param {number} w @param {number} h @param {string} seed */
function plan(w, h, seed) {
  const st = { rng: seedFrom(`${seed}|map:islands`) };
  return layout(w, h, () => rand(st));
}

test('islands: passes the shared map rules', () => {
  checkMap(islands);
});

test('islands: walls 8-22%, no gold, no wall frame on the outer edge', () => {
  for (const { w, h } of CASES)
    for (const seed of ['alpha', 'bravo']) {
      const t = build(islands, w, h, seed);
      const wall = share(t, WALL);
      assert.ok(wall >= 0.08 && wall <= 0.22, `${w}x${h} ${seed}: walls ${(wall * 100).toFixed(1)}%`);
      assert.equal(share(t, GOLD), 0);
      // Channels end at the edge, but there is no wall frame: each edge is mostly land.
      let edge = 0;
      for (let x = 0; x < w; x++) edge += t[x] + t[(h - 1) * w + x];
      for (let y = 0; y < h; y++) edge += t[y * w] + t[y * w + w - 1];
      assert.ok(edge < (w + h) * 2 * 0.3, `${w}x${h} ${seed}: edge has ${edge} wall tiles`);
    }
});

test('islands: grid size per map size', () => {
  for (const { w, h, nx, ny } of CASES) {
    const L = plan(w, h, 'alpha');
    assert.equal(L.nx, nx, `${w}x${h} columns`);
    assert.equal(L.ny, ny, `${w}x${h} rows`);
    assert.ok(L.cw >= 3 && L.cw <= 5 && L.bw >= 4 && L.bw <= 6);
  }
});

test('islands: every neighbouring pair of islands has an open bridge of 4+ tiles', () => {
  for (const { w, h } of CASES)
    for (const seed of ['alpha', 'bravo', 'charlie']) {
      const L = plan(w, h, seed);
      const t = build(islands, w, h, seed);
      const half = Math.floor(L.cw / 2);
      /** @param {number[]} cross @param {number} len */
      const segs = (cross, len) => {
        const edges = [0, ...cross, len];
        return edges.slice(0, -1).map((s, j) => [s, edges[j + 1]]);
      };
      for (const [axis, chans, cross, len] of /** @type {const} */ ([
        ['v', L.vx, L.hy, h],
        ['h', L.hy, L.vx, w],
      ]))
        for (let ch = 0; ch < chans.length; ch++)
          for (const [s0, s1] of segs(cross, len)) {
            const mine = L.bridges.filter((b) => b.axis === axis && b.ch === ch && b.a >= s0 && b.a + b.len <= s1);
            assert.ok(mine.length >= 1, `${w}x${h} ${seed}: bridge on ${axis}${ch} segment ${s0}-${s1}`);
            for (const b of mine) {
              assert.ok(b.len >= 4 && b.len <= 6);
              // Not at an island corner: clear of the crossing channels and the map edge.
              assert.ok(b.a - s0 >= 3 && s1 - (b.a + b.len) >= 3, 'bridge away from corners');
              // Open tiles all the way across the channel.
              for (let k = b.a; k < b.a + b.len; k++)
                for (let d = -half - L.amp; d <= half + L.amp; d++) {
                  const x = axis === 'v' ? chans[ch] + d : k;
                  const y = axis === 'v' ? k : chans[ch] + d;
                  assert.notEqual(t[y * w + x], WALL, `${w}x${h} ${seed}: bridge tile ${x},${y} open`);
                }
            }
          }
    }
});

test('islands: the centre line of each channel is mostly water', () => {
  for (const { w, h } of CASES)
    for (const seed of ['alpha', 'bravo']) {
      const L = plan(w, h, seed);
      const t = build(islands, w, h, seed);
      L.vx.forEach((c, i) => {
        let n = 0;
        for (let y = 0; y < h; y++) if (t[y * w + c + L.vOff[i][y]] === WALL) n++;
        assert.ok(n / h >= 0.7, `${w}x${h} ${seed}: v${i} ${n}/${h}`);
      });
      L.hy.forEach((c, j) => {
        let n = 0;
        for (let x = 0; x < w; x++) if (t[(c + L.hOff[j][x]) * w + x] === WALL) n++;
        assert.ok(n / w >= 0.7, `${w}x${h} ${seed}: h${j} ${n}/${w}`);
      });
    }
});

test('islands: channels are continuous water except at the bridges', () => {
  for (const { w, h } of CASES)
    for (const seed of ['alpha', 'bravo', 'charlie']) {
      const L = plan(w, h, seed);
      const t = build(islands, w, h, seed);
      const check = (/** @type {'v'|'h'} */ axis, /** @type {number} */ ch, /** @type {number} */ len, /** @type {(k: number) => number} */ at) => {
        let open = 0;
        for (let k = 0; k < len; k++) if (t[at(k)] !== WALL) open++;
        const n = L.bridges.filter((b) => b.axis === axis && b.ch === ch).length;
        assert.ok(open <= n * L.bw + 2, `${w}x${h} ${seed}: ${axis}${ch} has ${open} open centre tiles for ${n} bridges`);
      };
      L.vx.forEach((c, i) => check('v', i, h, (y) => y * w + c + L.vOff[i][y]));
      L.hy.forEach((c, j) => check('h', j, w, (x) => (c + L.hOff[j][x]) * w + x));
    }
});

test('islands: bots play without dying on the walls', () => {
  const r = botTrial('islands', { w: 100, h: 100, heads: 12, ticks: 3000 });
  assert.ok(r.deaths > 0 && r.captured > 0, `deaths ${r.deaths}, captured ${r.captured}`);
  assert.ok(r.wallDeaths <= r.deaths * 0.05, `wall deaths ${r.wallDeaths} of ${r.deaths}`);
});
