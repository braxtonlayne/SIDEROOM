// @ts-check
// Draft Night on screen: the round chip says where the run is ("Run 1 · round 2/3"). The keeper pick, the set
// chips and the run standings are in hud.js.

import { RUN_ROUNDS } from '../../shared/modes/draft.js';

/** @type {import('./index.js').ModeView} */
export default {
  chip(world) {
    const md = /** @type {any} */ (world.round?.md);
    if (!md || typeof md.leg !== 'number' || world.round?.phase !== 'play') return '';
    // Short: it shares a phone-wide chip with the mode name and the clock.
    return (md.runN > 1 ? `Run ${md.runN} · ` : '') + `Round ${md.leg}/${RUN_ROUNDS}`;
  },
};
