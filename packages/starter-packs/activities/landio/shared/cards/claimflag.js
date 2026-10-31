// @ts-check
// Claim Flag (item, common): claim the 7×7 around your head (radius 3; it was 5×5, about 40 tiles a game at 5×5), neutral tiles only, if it touches your land.
// Pure economy: it never takes enemy land, never touches trails and never encloses anyone.

import { square, setBase } from './util.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

const R = 3;

/** The tiles a flag at p's head would claim, or [] when the area does not touch p's land. @param {GameState} s @param {Player} p */
function claimable(s, p) {
  const area = square(s, p.x, p.y, R);
  // It must touch p's land: a tile of the area or right next to it is p's.
  const near = square(s, p.x, p.y, R + 1);
  if (!near.some((i) => s.base[i] === p.id)) return [];
  return area.filter((i) => s.base[i] === 0 && s.trail[i] === 0);
}

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'claimflag',
  name: 'Claim Flag',
  text: 'Claim the 7×7 around you (neutral land only). Must touch your land.',
  kind: 'item',
  rarity: 'common',
  maxLevel: 1,
  use(s, p) {
    const tiles = claimable(s, p);
    if (!tiles.length) return false;
    for (const i of tiles) setBase(s, i, p.id);
    p.captured += tiles.length;
    s.events.push({ k: 'fx', card: 'claimflag', id: p.id, x: p.x, y: p.y, n: tiles.length });
    return true;
  },
  botScore(s, p) {
    return claimable(s, p).length >= 10 ? 0.7 : 0;
  },
};
