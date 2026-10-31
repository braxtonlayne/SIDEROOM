// @ts-check
// Bloodlust (passive, rare): a kill recharges your active ability at once and gives a burst of speed: an
// extra tile on every third tick for the 21 ticks after the kill (the effect ends on kill + 21, exclusive).
// The kill tick itself was already moved, so the burst covers the 20 ticks after it.

/** Ticks the burst lasts after a kill (exclusive end, as every timed effect). */
export const BLOODLUST_TICKS = 21;

/** @type {import('../cards.js').CardDef} */
export default {
  id: 'bloodlust',
  name: 'Bloodlust',
  text: 'Each kill recharges your ability and gives you a burst of speed.',
  kind: 'passive',
  rarity: 'rare',
  maxLevel: 1,
  onKill(s, killer, _victim, level) {
    if (level <= 0) return;
    if (killer.cards.active) killer.cards.active.ready = s.tick;
    killer.fx.bloodlust = s.tick + BLOODLUST_TICKS;
    s.events.push({ k: 'fx', card: 'bloodlust', id: killer.id, x: killer.x, y: killer.y });
  },
  speed: (_level, fx, tick) => ((fx.bloodlust ?? 0) > tick && tick % 3 === 0 ? 1 : 0),
};
