// @ts-check
// Draft Night: every life starts with three card drafts in a row (the next one appears once you pick from the
// current one, or it times out), every card equally likely whatever its rarity. Kills give no cards.

import { offerCards } from '../cards.js';

/** Drafts per life. */
export const DRAFTS = 3;

/**
 * Offer `p` its next draft with every card equally likely (the engine's sandbox weights, switched on just for
 * this call: the simulation is single threaded, so nothing else sees the flag).
 * @param {import('../types.js').GameState} s @param {import('../types.js').Player} p
 */
function offerNext(s, p) {
  p.fx.drafted = (p.fx.drafted ?? 0) + 1;
  const was = s.cfg.sandbox;
  s.cfg.sandbox = true;
  try {
    offerCards(s, p, false);
  } finally {
    s.cfg.sandbox = was;
  }
}

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'draft',
  name: 'Draft Night',
  blurb: 'Pick three cards at the start. No cards from kills.',
  roundTicks: 5 * 60 * 7,
  respawn: true,
  cards: 'start',
  // p.fx is wiped on every placement and death, so `fx.drafted` counts the drafts offered this life.
  onSpawn(s, p) {
    offerNext(s, p);
  },
  tick(s) {
    for (const p of s.players)
      if (p.alive && !p.offers.length && (p.fx.drafted ?? 0) < DRAFTS) offerNext(s, p);
  },
};
