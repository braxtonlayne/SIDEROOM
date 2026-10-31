// @ts-check
// Grenade (item, common): thrown 6 tiles ahead, it blasts 5 ticks later (0.7 s) over the radius-2 diamond.
// The fuse is short because the bot aim predicts heads only as far as the fuse: at 7 ticks the predicted
// hits were rare (4 grenade deaths in 30 games against 36 at 5; see docs/measurements/card-balance.json).
// Enemy land in the blast goes neutral (your own is spared), trails through it are cut, and every head within
// 1 tile dies unless it stands on its own base.

import { DX, DY } from '../grid.js';
import { diamond, cutTrail, clearBase, kill, playerById, addObject } from './util.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */
/** @typedef {import('../types.js').WorldObject} WorldObject */

const THROW = 6;
const FUSE = 5;
const BLAST = 2;
/** Manhattan radius within which a head dies; BLAST is the land and trail radius. */
const KILL = 1;

/** The tile a grenade thrown by p lands on: THROW tiles ahead, clamped to the grid. p.dir must be >= 0. @param {GameState} s @param {Player} p */
function landing(s, p) {
  const { w, h } = s.cfg;
  const x = Math.min(w - 1, Math.max(0, p.x + DX[p.dir] * THROW));
  const y = Math.min(h - 1, Math.max(0, p.y + DY[p.dir] * THROW));
  return { x, y };
}

/** @param {GameState} s @param {WorldObject} o */
function explode(s, o) {
  const { w } = s.cfg;
  const tid = o.owner;
  const thrower = playerById(s, tid) ?? null;
  const blast = diamond(s, o.x, o.y, BLAST);
  const inBlast = new Set(blast);

  // A head standing on its own base in the blast keeps that tile: clearing it would leave the head on neutral
  // land with no trail, which the simulation never allows.
  /** @type {Set<number>} */
  const kept = new Set();
  for (const q of s.players) {
    if (!q.alive) continue;
    const h = q.y * w + q.x;
    if (inBlast.has(h) && s.base[h] === q.id) kept.add(h);
  }
  for (const i of blast) {
    const owner = s.base[i];
    if (owner && owner !== tid && !kept.has(i)) clearBase(s, i);
  }

  for (const q of s.players) if (q.trail.some((i) => inBlast.has(i))) cutTrail(s, q, inBlast);

  // Decide who dies from the state after the land and trails changed; the heads' own bases are kept above.
  /** @type {Player[]} */
  const victims = [];
  for (const q of s.players) {
    if (!q.alive || Math.abs(q.x - o.x) + Math.abs(q.y - o.y) > KILL) continue;
    if (s.base[q.y * w + q.x] === q.id) continue;
    victims.push(q);
  }
  for (const q of victims) {
    const killer = q !== thrower && thrower?.alive ? thrower : null;
    kill(s, q, 'grenade', killer);
  }

  const ev = { k: 'fx', card: 'grenade', id: tid, x: o.x, y: o.y, phase: 'boom' };
  s.events.push(ev);
}

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'grenade',
  name: 'Grenade',
  text: 'Q/E: throw 6 tiles ahead. After 0.7 s it blasts radius 2: enemy land goes neutral, trails are cut, heads within 1 tile die.',
  kind: 'item',
  rarity: 'common',
  maxLevel: 1,
  use(s, p) {
    if (p.dir < 0) return false;
    const { x, y } = landing(s, p);
    // Pushed directly rather than via addObject(): cards.js imports this module, so importing it back is a cycle.
    addObject(s, { kind: 'grenade', owner: p.id, x, y, dir: p.dir, until: s.tick + FUSE + 1, data: { fuse: s.tick + FUSE } });
    // `phase` is read by client/fx/grenade.js; GameEvent does not declare it, so the event goes in through a variable.
    const ev = { k: 'fx', card: 'grenade', id: p.id, x, y, phase: 'throw' };
    s.events.push(ev);
    return true;
  },
  objectTick(s, o) {
    if (o.dead || s.tick < (o.data?.fuse ?? 0)) return;
    o.dead = true;
    explode(s, o);
  },
  botScore(s, p) {
    if (p.dir < 0) return 0;
    const { w, h } = s.cfg;
    const { x, y } = landing(s, p);
    // The blast comes FUSE ticks later, so aim at where an enemy head will be then: a head that keeps going
    // straight. A head on its own base then is safe from the blast, so it is not a target.
    for (const q of s.players) {
      if (q === p || !q.alive) continue;
      const k = q.dir >= 0 && !q.stopped ? FUSE : 0;
      const fx = q.x + DX[Math.max(0, q.dir)] * k;
      const fy = q.y + DY[Math.max(0, q.dir)] * k;
      if (fx < 0 || fy < 0 || fx >= w || fy >= h) continue;
      if (s.base[fy * w + fx] === q.id) continue;
      if (Math.abs(fx - x) + Math.abs(fy - y) <= KILL) return 0.8;
    }
    // Not a kill, but a blast through an enemy's trail cuts it back and costs them the loop.
    for (const i of diamond(s, x, y, BLAST)) {
      const t = s.trail[i];
      if (t && t !== p.id) return 0.65;
    }
    return 0;
  },
};
