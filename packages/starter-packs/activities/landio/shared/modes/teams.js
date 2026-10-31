// @ts-check
// Teams: two teams (three in a busy arena, four in a crowded one). Teammates cannot cut each other and their
// land adds up; the engine does the mechanics (allies in movement and capture, team colours, team results).
// This file picks the sides and gives bots a rallying point so a team forms a front.

/** How often (ticks) a team's centre of heads is recomputed for bots. */
const CENTRE_EVERY = 35;
/** Farther than this (Manhattan tiles) from the team centre, a bot is pulled back towards it. */
const FAR = 25;

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

/** How many teams this arena plays with: 2, 3 from 18 heads, 4 from 28. @param {GameState} s */
export function teamCount(s) {
  const n = s.cfg.targetHeads;
  return n >= 28 ? 4 : n >= 18 ? 3 : 2;
}

/**
 * The team for `p`: a human keeps the side they had earlier in this round; otherwise the smallest living team
 * among the first teamCount(s), the lowest number on a tie. @param {GameState} s @param {Player} p
 */
function pickTeam(s, p) {
  const n = teamCount(s);
  const uids = /** @type {string[]} */ (s.md.uids ?? (s.md.uids = []));
  const sides = /** @type {number[]} */ (s.md.sides ?? (s.md.sides = []));
  if (p.uid) {
    const i = uids.indexOf(p.uid);
    if (i >= 0 && sides[i] >= 1 && sides[i] <= n) return sides[i];
  }
  let best = 1;
  let bestCount = Infinity;
  for (let t = 1; t <= n; t++) {
    let c = 0;
    for (const q of s.players) if (q !== p && q.alive && q.team === t) c++;
    if (c < bestCount) {
      best = t;
      bestCount = c;
    }
  }
  if (p.uid) {
    uids.push(p.uid);
    sides.push(best);
  }
  return best;
}

/** Recompute every team's centre (mean head position of its living members) into s.md. @param {GameState} s */
function refreshCentres(s) {
  const sx = [0, 0, 0, 0, 0];
  const sy = [0, 0, 0, 0, 0];
  const c = [0, 0, 0, 0, 0];
  for (const q of s.players) {
    if (!q.alive || q.team < 1 || q.team > 4) continue;
    sx[q.team] += q.x;
    sy[q.team] += q.y;
    c[q.team]++;
  }
  s.md.cx = sx.map((v, t) => (c[t] ? Math.round(v / c[t]) : -1));
  s.md.cy = sy.map((v, t) => (c[t] ? Math.round(v / c[t]) : -1));
  s.md.cAt = s.tick;
}

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'teams',
  name: 'Teams',
  blurb: 'Two teams. Teammates cannot cut each other; team land adds up.',
  roundTicks: 5 * 60 * 7,
  respawn: true,
  teams: 4,
  pickTeam,
  botGoal(s, p) {
    if (p.team < 1) return null;
    if (s.md.cAt === undefined || s.tick - /** @type {number} */ (s.md.cAt) >= CENTRE_EVERY) refreshCentres(s);
    const x = /** @type {number[]} */ (s.md.cx)[p.team];
    const y = /** @type {number[]} */ (s.md.cy)[p.team];
    if (x < 0 || Math.abs(p.x - x) + Math.abs(p.y - y) <= FAR) return null;
    return { x, y };
  },
};
