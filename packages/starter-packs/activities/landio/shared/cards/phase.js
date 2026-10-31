// @ts-check
// Phase (active, epic): for 1 s (7 ticks) your head crosses any trail unharmed. The trail's owner does not
// die, and the engine cuts the trail (enemy) or pinches the loop (your own) at the crossing (D-39). Stepping
// onto an enemy's HEAD still kills that enemy: immunity never covers a head. Works from p.fx, so no level.

import { DX, DY, playerById } from '../grid.js';

export const PHASE_TICKS = 7;

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'phase',
  name: 'Phase',
  text: 'Shift: for 1 s, cross any trail unharmed (stepping onto a head still kills it). Recharges in 20 s.',
  kind: 'active',
  rarity: 'epic',
  maxLevel: 1,
  cooldown: () => 140,
  use(s, p) {
    p.fx.phase = s.tick + PHASE_TICKS;
    return true;
  },
  trailImmune(s, mover) {
    return (mover.fx.phase ?? 0) > s.tick;
  },
  predictUse(_level, fx, at) {
    fx.phase = at + PHASE_TICKS;
  },
  botScore(s, p) {
    if (p.dir < 0) return 0;
    const { w, h } = s.cfg;
    const x = p.x + DX[p.dir];
    const y = p.y + DY[p.dir];
    if (x < 0 || y < 0 || x >= w || y >= h) return 0;
    const i = y * w + x;
    const owner = s.trail[i];
    if (!owner) return 0;
    // An enemy's head is not a trail to phase through: stepping on it kills that enemy anyway.
    const v = playerById(s, owner);
    if (v && v !== p && v.trail[v.trail.length - 1] === i) return 0;
    return 0.95;
  },
};
