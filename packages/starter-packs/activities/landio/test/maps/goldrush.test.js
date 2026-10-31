// @ts-check
// Gold Rush (gold 8-14% on square arenas of 100+; small and stretched arenas have less room, so 5%+): the shared map rules, then what makes it Gold Rush (centre field, mirrored satellites, cover walls).
import assert from 'node:assert/strict';
import goldrush from '../../shared/maps/goldrush.js';
import { WALL, GOLD } from '../../shared/grid.js';
import { build, botTrial, checkMap, share } from './helpers.js';

const SIZES = [[100, 100], [200, 200], [130, 130], [150, 120], [110, 130], [60, 50], [80, 80]];
const SEEDS = ['alpha', 'bravo', 'charlie'];

/** Number of 4-connected gold components. @param {Uint8Array} t @param {number} w @param {number} h */
function goldFields(t, w, h) {
  const seen = new Uint8Array(w * h);
  let n = 0;
  for (let s = 0; s < w * h; s++) {
    if (seen[s] || t[s] !== GOLD) continue;
    n++;
    const q = [s];
    seen[s] = 1;
    while (q.length) {
      const i = /** @type {number} */ (q.pop());
      const x = i % w;
      const y = (i - x) / w;
      for (const [nx, ny] of [[x + 1, y], [x - 1, y], [x, y + 1], [x, y - 1]]) {
        if (nx < 0 || ny < 0 || nx >= w || ny >= h) continue;
        const j = ny * w + nx;
        if (!seen[j] && t[j] === GOLD) {
          seen[j] = 1;
          q.push(j);
        }
      }
    }
  }
  return n;
}

test('goldrush: passes the shared map rules', () => {
  checkMap(goldrush);
});

test('goldrush: gold share, centre field, symmetry, field count, walls clear of gold', () => {
  for (const [w, h] of SIZES)
    for (const seed of SEEDS) {
      const where = `${w}x${h} ${seed}`;
      const t = build(goldrush, w, h, seed);
      const g = share(t, GOLD);
      assert.ok(g >= (w === h && w >= 100 ? 0.08 : 0.05) && g <= 0.14, `${where}: gold ${(g * 100).toFixed(1)}%`);
      assert.ok(share(t, WALL) <= 0.08, `${where}: walls ${(share(t, WALL) * 100).toFixed(1)}%`);
      assert.equal(t[(h >> 1) * w + (w >> 1)], GOLD, `${where}: centre is gold`);
      for (let y = 0; y < h; y++)
        for (let x = 0; x < w; x++) assert.equal(t[y * w + x], t[(h - 1 - y) * w + (w - 1 - x)], `${where}: symmetric at ${x},${y}`);
      const fields = goldFields(t, w, h);
      if (Math.min(w, h) < 100) assert.equal(fields, 5, `${where}: 5 fields`);
      else assert.ok(fields >= 7 && fields <= 9, `${where}: ${fields} fields`);
      let walls = 0;
      for (let y = 0; y < h; y++)
        for (let x = 0; x < w; x++) {
          if (t[y * w + x] !== WALL) continue;
          walls++;
          for (let dy = -3; dy <= 3; dy++)
            for (let dx = -3; dx <= 3; dx++) {
              const nx = x + dx;
              const ny = y + dy;
              if (nx >= 0 && ny >= 0 && nx < w && ny < h) assert.notEqual(t[ny * w + nx], GOLD, `${where}: wall ${x},${y} near gold`);
            }
        }
      assert.ok(walls > 0, `${where}: has cover walls`);
    }
});

test('goldrush: bots play without dying on walls', () => {
  const r = botTrial('goldrush', { w: 100, h: 100, heads: 12, ticks: 3000 });
  assert.ok(r.deaths > 0, 'some deaths');
  assert.ok(r.captured > 0, 'some captures');
  assert.ok(r.wallDeaths <= r.deaths * 0.05, `wall deaths ${r.wallDeaths}/${r.deaths}`);
  console.log(`goldrush trial: deaths ${r.deaths}, wall ${r.wallDeaths}, captured ${r.captured}`);
});
