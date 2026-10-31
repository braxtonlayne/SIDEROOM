// @ts-check
// infection: the round chip shows the two sides and, for an infected local player, a warning.

/** @type {import('./index.js').ModeView} */
export default {
  chip(world, me) {
    let clean = 0;
    let virus = 0;
    for (const [id, r] of world.roster) {
      if (!world.stats.get(id)?.alive) continue;
      if (r.team === 2) virus++;
      else if (r.team === 1) clean++;
    }
    const mine = world.roster.get(me)?.team === 2 ? 'You are infected · ' : '';
    return `${mine}Clean ${clean} · Virus ${virus}`;
  },
};
