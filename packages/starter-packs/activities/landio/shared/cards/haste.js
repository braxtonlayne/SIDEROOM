// @ts-check
// Haste (passive, common): an extra tile every 6 ticks (II: every 3). Pure speed: the simulation and the
// client both read it from speed(), so it is a function of the tick only.

/** Ticks between extra tiles, by level (index 0 = no haste). */
const PERIOD = [0, 6, 3];

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'haste',
  name: 'Haste',
  text: 'A little faster: an extra tile every 6 ticks (II: every 3).',
  kind: 'passive',
  rarity: 'common',
  maxLevel: 2,
  speed: (level, _fx, tick) => (level > 0 && tick % PERIOD[level] === 0 ? 1 : 0),
};
