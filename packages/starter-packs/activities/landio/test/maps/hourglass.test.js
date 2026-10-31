// @ts-check
// Hourglass: the shared map rules, plus its own shape: mirror symmetric halves joined by one waist.
import assert from 'node:assert/strict';
import hourglass from '../../shared/maps/hourglass.js';
import { WALL, GOLD } from '../../shared/grid.js';
import { botTrial, build, checkMap, share } from './helpers.js';

const SIZES = [
  { w: 100, h: 100 },
  { w: 60, h: 50 },
  { w: 50, h: 60 },
  { w: 200, h: 120 },
  { w: 40, h: 40 },
];

test('hourglass: passes the shared map rules', () => {
  checkMap(hourglass);
});

test('hourglass: bots rarely die on walls', () => {
  const r = botTrial('hourglass', { w: 100, h: 100, heads: 12, ticks: 3000 });
  assert.ok(r.deaths > 0, 'some deaths');
  assert.ok(r.captured > 0, 'some captures');
  assert.ok(r.wallDeaths <= r.deaths * 0.05, `wall deaths ${r.wallDeaths} of ${r.deaths}`);
});

/** Which mirror symmetries hold. @param {Uint8Array} t @param {number} w @param {number} h */
function axisOf(t, w, h) {
  let xs = true;
  let ys = true;
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
    if (t[y * w + x] !== t[y * w + (w - 1 - x)]) xs = false;
    if (t[y * w + x] !== t[(h - 1 - y) * w + x]) ys = false;
  }
  return { xs, ys };
}

test('hourglass: mirror symmetric, no gold, walls 15-30%', () => {
  for (const { w, h } of SIZES)
    for (const seed of ['alpha', 'bravo']) {
      const t = build(hourglass, w, h, seed);
      const { xs, ys } = axisOf(t, w, h);
      if (w > h) assert.ok(xs, `${w}x${h}: mirrored left/right`);
      else if (h > w) assert.ok(ys, `${w}x${h}: mirrored top/bottom`);
      else assert.ok(xs || ys, `${w}x${h}: mirrored`);
      assert.equal(share(t, GOLD), 0);
      const wl = share(t, WALL);
      assert.ok(wl >= 0.15 && wl <= 0.3, `${w}x${h} ${seed}: walls ${wl}`);
    }
});

test('hourglass: one waist run on the centre line, cutting the long axis', () => {
  for (const { w, h } of SIZES)
    for (const seed of ['alpha', 'bravo']) {
      const t = build(hourglass, w, h, seed);
      const alongX = w > h || (w === h && isXWaist(t, w, h));
      const L = alongX ? w : h;
      const C = alongX ? h : w;
      for (const a of new Set([Math.floor((L - 1) / 2), Math.ceil((L - 1) / 2)])) {
        let runs = 0;
        let width = 0;
        let prev = true;
        for (let c = 0; c < C; c++) {
          const open = t[alongX ? c * w + a : a * w + c] !== WALL;
          if (open) {
            width++;
            if (prev) runs++;
          }
          prev = !open;
        }
        assert.equal(runs, 1, `${w}x${h}: one open run on the centre line`);
        assert.ok(width >= Math.max(7, 0.18 * C - 1e-9), `${w}x${h}: waist ${width} wide enough`);
        assert.ok(width <= Math.max(7, 0.22 * C + 1e-9), `${w}x${h}: waist ${width} not too wide`);
      }
    }
});

/** For a square map: the waist cuts x when the centre column has more wall than the centre row. @param {Uint8Array} t @param {number} w @param {number} h */
function isXWaist(t, w, h) {
  let wallsCol = 0;
  let wallsRow = 0;
  const m = Math.floor(w / 2);
  for (let i = 0; i < w; i++) {
    if (t[i * w + m] === WALL) wallsCol++;
    if (t[m * w + i] === WALL) wallsRow++;
  }
  return wallsCol > wallsRow;
}

test('hourglass: each half has plenty of open ground', () => {
  for (const { w, h } of SIZES) {
    const t = build(hourglass, w, h, 'alpha');
    const alongX = w > h || (w === h && isXWaist(t, w, h));
    const L = alongX ? w : h;
    let a = 0;
    let b = 0;
    for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) {
      if (t[y * w + x] === WALL) continue;
      const k = alongX ? x : y;
      if (k < L / 2) a++;
      else b++;
    }
    assert.equal(a, b, `${w}x${h}: halves equal`);
    assert.ok(a > 0.3 * w * h, `${w}x${h}: half has ${a} open tiles`);
  }
});
