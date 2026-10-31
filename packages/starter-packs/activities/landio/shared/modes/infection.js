// @ts-check
// Infection: one player starts as the virus (team 2); everyone else is Clean (team 1). A Virus player who kills a
// Clean player infects them: a human comes back on the Virus side, and bots that join later are Virus with the
// current virus share as their chance. The virus wins when no Clean player is alive (after 30 s); the Clean
// win if any Clean is alive when the five minutes are up.

import { rand } from '../rng.js';
import { LOOK_COLORS } from '../look.js';
import { TEAMS } from './index.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

const CLEAN = 1;
const VIRUS = 2;
/** The virus cannot win before this many ticks (30 s). */
const MIN_TICKS = 30 * 7;
/** With no living virus this long (10 s), a clean player is infected even if infected humans could respawn. */
const NO_VIRUS_TICKS = 10 * 7;

/** Move `p` to `team` and recolour it: the team colour plus a pattern per teammate. @param {GameState} s @param {Player} p @param {number} team */
export function setSide(s, p, team) {
  p.team = team;
  let mates = 0;
  for (const q of s.players) if (q !== p && q.team === team) mates++;
  p.look = TEAMS[team - 1].color + LOOK_COLORS * (mates % 4);
}

/** @param {Player} p */
const keyOf = (p) => p.uid ?? `#${p.id}`;

/** @param {GameState} s @returns {string[]} */
function infected(s) {
  if (!Array.isArray(s.md.inf)) s.md.inf = [];
  return /** @type {string[]} */ (s.md.inf);
}

/** Living players on each side. @param {GameState} s */
function counts(s) {
  let clean = 0;
  let virus = 0;
  for (const p of s.players)
    if (p.alive) {
      if (p.team === VIRUS) virus++;
      else if (p.team === CLEAN) clean++;
    }
  return { clean, virus };
}

/** Make one living Clean player (a random human if any, else a random bot) the virus. @param {GameState} s */
function infectOne(s) {
  const live = s.players.filter((p) => p.alive && p.team === CLEAN);
  if (!live.length) return false;
  const humans = live.filter((p) => p.kind === 'human');
  const pool = humans.length ? humans : live;
  const p = pool[Math.floor(rand(s) * pool.length)];
  if (p.kind === 'human') infected(s).push(keyOf(p));
  setSide(s, p, VIRUS);
  s.md.go = 1;
  return true;
}

/** The winner: -2 (Virus) unless a Clean player is alive, -1 (Clean). @param {GameState} s */
export function winner(s) {
  return counts(s).clean > 0 ? -CLEAN : -VIRUS;
}

/** @type {import('./index.js').ModeDef} */
export default {
  id: 'infection',
  name: 'Infection',
  blurb: 'Get caught by the virus and you join it. Last clean player wins.',
  roundTicks: 5 * 60 * 7,
  respawn: true,
  teams: 2,
  winner,

  setup(s) {
    infectOne(s);
  },

  pickTeam(s, p) {
    return p.kind === 'human' && infected(s).includes(keyOf(p)) ? VIRUS : CLEAN;
  },

  onSpawn(s, p) {
    if (p.kind !== 'bot' || !s.md.go) return;
    const { clean, virus } = counts(s);
    const share = virus / Math.max(1, clean + virus);
    if (rand(s) < share) setSide(s, p, VIRUS);
  },

  onKill(s, victim, killer) {
    if (!killer || killer.team !== VIRUS || victim.team !== CLEAN) return;
    if (victim.kind === 'human') {
      const list = infected(s);
      if (!list.includes(keyOf(victim))) list.push(keyOf(victim));
      setSide(s, victim, VIRUS);
    }
  },

  tick(s) {
    if (!s.md.go) {
      // Nobody was there at setup (or the virus died out): infect someone.
      if (s.players.length) infectOne(s);
    } else if (counts(s).virus === 0) {
      // The virus died out. Infected humans waiting to respawn bring it back; if none has for 10 s (they left, or
      // are sitting on the summary), someone clean is infected so the round keeps going.
      const since = Number(s.md.none ?? s.tick);
      if (s.md.none === undefined) s.md.none = s.tick;
      if (infected(s).length === 0 || s.tick - since >= NO_VIRUS_TICKS) {
        infectOne(s);
        delete s.md.none;
      }
    } else if (s.md.none !== undefined && counts(s).virus > 0) delete s.md.none;
    if (!s.md.had && counts(s).clean > 0) s.md.had = 1;
  },

  roundOver(s) {
    if (!s.md.had || s.tick - s.round.start < MIN_TICKS) return false;
    const c = counts(s);
    return c.clean === 0 && c.virus > 0;
  },

  botGoal(s, p) {
    if (p.team !== VIRUS) return null;
    let best = null;
    let bd = Infinity;
    for (const q of s.players) {
      if (!q.alive || q.team !== CLEAN) continue;
      const d = Math.abs(q.x - p.x) + Math.abs(q.y - p.y);
      if (d < bd) {
        bd = d;
        best = q;
      }
    }
    return best ? { x: best.x, y: best.y } : null;
  },
};
