// @ts-check
// Donut: the shared map rules, plus a solid void in the middle ringed by open ground and a thin gold band.
import assert from 'node:assert/strict';
import donut from '../../shared/maps/donut.js';
import { OPEN, WALL, GOLD } from '../../shared/grid.js';
import { build, checkMap, share } from './helpers.js';

const SIZES = [[40, 40], [100, 100], [200, 200], [60, 50], [120, 80]];

test('donut: passes the shared map rules', () => {
  checkMap(donut);
});

test('donut: bots play without dying on walls', async () => {
  const { botTrial } = await import('./helpers.js');
  const r = botTrial('donut', { w: 100, h: 100, heads: 12, ticks: 3000 });
  assert.ok(r.deaths > 0, 'some deaths');
  assert.ok(r.captured > 0, 'some captures');
  assert.ok(r.wallDeaths <= r.deaths * 0.05, `wall deaths ${r.wallDeaths} of ${r.deaths}`);
});

test('donut: centre is wall; each of 8 directions goes void, then a wide open ring, then edge or outer wall', () => {
  for (const [w, h] of SIZES) {
    const t = build(donut, w, h, 'alpha');
    const cx = Math.floor(w / 2);
    const cy = Math.floor(h / 2);
    assert.equal(t[cy * w + cx], WALL, `${w}x${h}: centre is wall`);
    for (const [dx, dy] of [[1, 0], [-1, 0], [0, 1], [0, -1], [1, 1], [1, -1], [-1, 1], [-1, -1]]) {
      let x = cx;
      let y = cy;
      const at = () => t[y * w + x];
      const inside = () => x >= 0 && y >= 0 && x < w && y < h;
      let voidLen = 0;
      while (inside() && at() === WALL) {
        voidLen++;
        x += dx;
        y += dy;
      }
      const where = `${w}x${h} dir ${dx},${dy}`;
      assert.ok(voidLen >= 0.2 * Math.min(w, h) * (dx && dy ? 0.7 : 1), `${where}: void ${voidLen}`);
      let ring = 0;
      while (inside() && at() !== WALL) {
        ring++;
        x += dx;
        y += dy;
      }
      assert.ok(ring >= (dx && dy ? 8 : 9), `${where}: ring only ${ring} wide`);
      if (dx && dy) assert.ok(!inside() || at() === WALL, `${where}: ends at edge or outer wall`);
    }
  }
});

test('donut: the outer corners are rounded off with wall', () => {
  for (const [w, h] of SIZES) {
    const t = build(donut, w, h, 'alpha');
    for (const [x, y] of [[0, 0], [w - 1, 0], [0, h - 1], [w - 1, h - 1]]) assert.equal(t[y * w + x], WALL, `${w}x${h} corner`);
    // Edge midpoints stay open ground.
    assert.notEqual(t[(h >> 1) * w], WALL);
    assert.notEqual(t[(w >> 1)], WALL);
  }
});

test('donut: gold is 3-6% and hugs the void', () => {
  for (const [w, h] of SIZES) {
    const t = build(donut, w, h, 'alpha');
    const g = share(t, GOLD);
    assert.ok(g >= 0.03 && g <= 0.06, `${w}x${h}: gold ${(g * 100).toFixed(1)}%`);
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        if (t[y * w + x] !== GOLD) continue;
        let near = false;
        const reach = Math.ceil(Math.max(1, Math.round(Math.min(w, h) * 0.26 / 13)) * Math.max(w, h) / Math.min(w, h)); // band width, stretched on the long axis
        for (let dy = -reach; dy <= reach && !near; dy++)
          for (let dx = -reach; dx <= reach; dx++) {
            const nx = x + dx;
            const ny = y + dy;
            // Wall that is not near the map edge is the void.
            if (nx >= 0 && ny >= 0 && nx < w && ny < h && t[ny * w + nx] === WALL) near = true;
          }
        assert.ok(near, `${w}x${h}: gold at ${x},${y} is far from the void`);
      }
  }
});

test('donut: symmetric left-right and top-bottom, walls 25-40%', () => {
  for (const [w, h] of SIZES) {
    const t = build(donut, w, h, 'alpha');
    for (let y = 0; y < h; y++)
      for (let x = 0; x < w; x++) {
        assert.equal(t[y * w + x], t[y * w + (w - 1 - x)], `${w}x${h} left-right`);
        assert.equal(t[y * w + x], t[(h - 1 - y) * w + x], `${w}x${h} top-bottom`);
      }
    const wl = share(t, WALL);
    assert.ok(wl >= 0.25 && wl <= 0.4, `${w}x${h}: walls ${(wl * 100).toFixed(1)}%`);
    assert.ok(share(t, OPEN) > 0.3);
  }
});
