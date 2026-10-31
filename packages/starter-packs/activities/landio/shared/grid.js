// @ts-check
// Grid helpers. Every write to `base`, `trail` or `terrain` goes through setBase/setTrail/setTerrain so land
// counts, bounding boxes and the dirty lists (what the server sends as a delta) stay right.

/** @typedef {import('./types.js').GameState} GameState */
/** @typedef {import('./types.js').Player} Player */

export const DX = [0, 1, 0, -1];
export const DY = [-1, 0, 1, 0];

/** @param {number} d */
export const opposite = (d) => (d + 2) & 3;

/** @param {GameState} s @param {number} x @param {number} y */
export const inBounds = (s, x, y) => x >= 0 && y >= 0 && x < s.cfg.w && y < s.cfg.h;

// Terrain (a map's layout; D-50). Open ground; a wall, which is like the map's edge (a head entering it dies,
// nobody owns it, it never closes a loop); gold, open ground worth more in modes that score it.
export const OPEN = 0;
export const WALL = 1;
export const GOLD = 2;

/** Whether a head may be on (x, y): on the grid and not a wall. @param {GameState} s @param {number} x @param {number} y */
export const passable = (s, x, y) => x >= 0 && y >= 0 && x < s.cfg.w && y < s.cfg.h && s.terrain[y * s.cfg.w + x] !== WALL;

/** @param {GameState} s @param {number} x @param {number} y */
export const idx = (s, x, y) => y * s.cfg.w + x;

/** @param {GameState} s @param {number} id @returns {Player|undefined} */
export function playerById(s, id) {
  // Few players (≤ 64): a scan is faster than keeping a map in sync.
  for (const p of s.players) if (p.id === id) return p;
  return undefined;
}

/** @param {GameState} s @param {number} i */
function markDirty(s, i) {
  if (!s.dirtyMark[i]) {
    s.dirtyMark[i] = 1;
    s.dirty.push(i);
  }
}

/** Set the base owner of tile `i`. @param {GameState} s @param {number} i @param {number} owner */
export function setBase(s, i, owner) {
  const old = s.base[i];
  if (old === owner) return;
  // Nobody owns a wall (a mode turning a tile to wall clears its owner first).
  if (owner && s.terrain[i] === WALL) return;
  const gold = s.terrain[i] === GOLD ? 1 : 0;
  if (old) {
    const p = playerById(s, old);
    if (p) {
      p.land--;
      p.gold -= gold;
    }
  }
  s.base[i] = owner;
  if (owner) {
    const p = playerById(s, owner);
    if (p) {
      p.land++;
      p.gold += gold;
      if (p.land > p.maxLand) p.maxLand = p.land;
      const x = i % s.cfg.w;
      const y = (i - x) / s.cfg.w;
      const b = p.bbox;
      if (b.length === 0) b.push(x, y, x, y);
      else {
        if (x < b[0]) b[0] = x;
        if (y < b[1]) b[1] = y;
        if (x > b[2]) b[2] = x;
        if (y > b[3]) b[3] = y;
      }
    }
  }
  markDirty(s, i);
}

/** Set the trail owner of tile `i`. @param {GameState} s @param {number} i @param {number} owner */
export function setTrail(s, i, owner) {
  if (s.trail[i] === owner) return;
  s.trail[i] = owner;
  markDirty(s, i);
}

/**
 * Change the terrain of tile `i` (a mode's shrinking border, say). Turning a tile into a wall clears its base
 * and trail; a head standing there is the caller's business. @param {GameState} s @param {number} i @param {number} t
 */
export function setTerrain(s, i, t) {
  const old = s.terrain[i];
  if (old === t) return;
  if (t === WALL) {
    setTrail(s, i, 0);
    setBase(s, i, 0);
  } else if (s.base[i] && (old === GOLD) !== (t === GOLD)) {
    const p = playerById(s, s.base[i]);
    if (p) p.gold += t === GOLD ? 1 : -1;
  }
  s.terrain[i] = t;
  s.terrainDirty.push(i);
}

/** The tiles whose terrain changed since the last call, and forget them. @param {GameState} s */
export function takeTerrainDirty(s) {
  const out = s.terrainDirty;
  s.terrainDirty = [];
  return out;
}

/** The tiles changed since the last call, and forget them. @param {GameState} s */
export function takeDirty(s) {
  const out = s.dirty;
  for (const i of out) s.dirtyMark[i] = 0;
  s.dirty = [];
  return out;
}
