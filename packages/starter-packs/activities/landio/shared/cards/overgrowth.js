// @ts-check
// Overgrowth (passive, rare): every loop you close also claims the ring of neutral land around what it
// captured, 1 tile wide (Overgrowth II: 2). Neutral tiles only; never trails.

import { setBase } from './util.js';

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'overgrowth',
  name: 'Overgrowth',
  text: 'Your captures also claim the neutral ring around them (II: 2 tiles wide).',
  kind: 'passive',
  rarity: 'rare',
  maxLevel: 2,
  onCapture(s, p, gained, level) {
    if (!level || !gained.length) return;
    const { w, h } = s.cfg;
    /** @type {Set<number>} */
    const ring = new Set();
    for (const i of gained) {
      const x = i % w;
      const y = (i - x) / w;
      for (let dy = -level; dy <= level; dy++)
        for (let dx = -level; dx <= level; dx++) {
          const tx = x + dx;
          const ty = y + dy;
          if (tx < 0 || ty < 0 || tx >= w || ty >= h) continue;
          const j = ty * w + tx;
          if (s.base[j] === 0 && s.trail[j] === 0) ring.add(j);
        }
    }
    for (const j of ring) setBase(s, j, p.id);
    p.captured += ring.size;
  },
};
