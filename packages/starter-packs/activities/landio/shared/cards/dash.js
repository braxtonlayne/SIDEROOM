// @ts-check
// Dash (active, common): move 2 tiles a tick for 1 s (7 ticks), then recharge 8 s (Dash II: 6 s). The trail
// is laid on every tile passed, so a dash is fast but exposed.

/** @typedef {import('../types.js').GameState} GameState */

export const DASH_TICKS = 7;

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'dash',
  name: 'Dash',
  text: 'Shift: double speed for 1 s. Recharges in 8 s (Dash II: 6 s).',
  kind: 'active',
  rarity: 'common',
  maxLevel: 2,
  cooldown: (level) => (level >= 2 ? 42 : 56),
  use(s, p) {
    if (p.dir < 0) return false;
    p.fx.dash = s.tick + DASH_TICKS;
    p.stopped = false;
    return true;
  },
  speed: (_level, fx, tick) => ((fx.dash ?? 0) > tick ? 1 : 0),
  predictUse(_level, fx, at) {
    fx.dash = at + DASH_TICKS;
  },
  botScore(s, p) {
    const m = p.bot;
    if (!m || !p.trail.length) return 0;
    if (m.mode === 'flee') return 0.9;
    if (m.mode === 'attack') return 0.75;
    return 0;
  },
};
