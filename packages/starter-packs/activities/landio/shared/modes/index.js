// @ts-check
// Game modes (D-51). A mode decides how a round runs and how it is won: its length, whether the dead come back,
// teams, how players are scored, and anything special it does each tick. The round lifecycle itself (start,
// end, the results screen, the reset) lives in shared/rules.js; a mode only fills in the hooks below. Modes
// import nothing from rules.js (rules.js imports them), only grid helpers and the card engine.

import classic from './classic.js';
import rounds from './rounds.js';
import teams from './teams.js';
import lastland from './lastland.js';
import shrink from './shrink.js';
import koth from './koth.js';
import infection from './infection.js';
import draft from './draft.js';

/** @typedef {import('../types.js').GameState} GameState */
/** @typedef {import('../types.js').Player} Player */

/**
 * @typedef {object} ModeDef
 * @property {string} id                  lower case, used in config and URLs
 * @property {string} name                shown to players
 * @property {string} blurb               one line for the mode picker
 * @property {number} roundTicks          how long a round lasts (0 = endless: no rounds, no results screen)
 * @property {boolean} respawn            whether the dead may play again during a round (else: the next round)
 * @property {number} [teams]             number of teams (2-4); 0 or absent = every player for themselves
 * @property {'kills' | 'start' | 'off'} [cards]  where cards come from: drafts on kills (default), three
 *   drafts at the start of each round (and none on kills), or no cards at all
 * @property {string} [scoreLabel]        what the leaderboard number means ('land' by default)
 * @property {(s: GameState) => void} [setup]   a round starts: the map is built and everyone is placed
 * @property {(s: GameState) => void} [tick]    every tick of a running round, after everyone moved
 * @property {(s: GameState, p: Player) => number} [score]  ranks players; default: land, gold counted 3x
 * @property {(s: GameState, a: Player, b: Player) => boolean} [allies]  default: same nonzero team
 * @property {(s: GameState) => boolean} [roundOver]  end the round early (say, one player left)
 * @property {(s: GameState, victim: Player, killer: Player | null, cause: string) => void} [onKill]  after a death
 * @property {(s: GameState, p: Player) => void} [onSpawn]  a player (human or bot) was just placed
 * @property {(s: GameState, p: Player) => {x: number, y: number} | null} [botGoal]  a spot bot `p` should grow its
 *   land towards (bots then pick loop directions that close in on it); null for none
 * @property {(s: GameState, a: Player, b: Player) => number} [rank]  orders the results (negative: a first);
 *   default: higher score first
 * @property {(s: GameState) => number} [winner]  who won the round: a player id, minus a team number, or 0 for
 *   nobody; default: the first in the results (teams: the team with the most points)
 * @property {(s: GameState) => boolean} [botsJoin]  whether a missing bot may join now (default: always)
 * @property {(s: GameState, p: Player) => number} [pickTeam]  the team for `p`, a player being placed (a new
 *   one, a respawn or a new round; p.uid identifies a human across lives); default: the smallest team
 */

/** @type {ModeDef[]} */
export const MODES = [classic, rounds, teams, lastland, shrink, koth, infection, draft];

/** @type {Record<string, ModeDef>} */
export const MODE_BY_ID = Object.fromEntries(MODES.map((m) => [m.id, m]));

/** The arena's mode (an unknown id is Classic). @param {{cfg: import('../types.js').Config}} s @returns {ModeDef} */
export const modeOf = (s) => MODE_BY_ID[s.cfg.mode] ?? classic;

/** Teams 1..4: a name and a colour (an index into shared/look.js COLORS). */
export const TEAMS = [
  { name: 'Red', color: 5 },
  { name: 'Blue', color: 4 },
  { name: 'Green', color: 2 },
  { name: 'Yellow', color: 3 },
];

/** Gold counts this many times in the default score. */
export const GOLD_VALUE = 3;

/** A player's score in the arena's mode. @param {GameState} s @param {Player} p */
export function scoreOf(s, p) {
  const m = modeOf(s);
  return m.score ? m.score(s, p) : p.land + (GOLD_VALUE - 1) * p.gold;
}

/** Whether a and b are on the same side (never true of a player and itself). @param {GameState} s @param {Player} a @param {Player} b */
export function allied(s, a, b) {
  if (a === b) return false;
  const m = modeOf(s);
  if (m.allies) return m.allies(s, a, b);
  return a.team > 0 && a.team === b.team;
}

/**
 * The ids whose land is home to `p`: itself, plus its teammates in team modes (a teammate's land closes a loop
 * like your own). @param {GameState} s @param {Player} p @returns {number[]}
 */
export function homeIds(s, p) {
  if (!p.team) return [p.id];
  const out = [p.id];
  for (const q of s.players) if (q !== p && q.team === p.team && allied(s, p, q)) out.push(q.id);
  return out;
}

/** Each arena's team by player id, rebuilt once per tick (a cache only: it holds nothing the state does not). */
const teamCache = new WeakMap();

/**
 * A fast test for "tile owner `o` is home to `p`" (`p` itself, or a teammate): bots ask it for every tile of
 * their path searches. @param {GameState} s @param {Player} p @returns {(owner: number) => boolean}
 */
export function isHomeOwner(s, p) {
  const id = p.id;
  if (!p.team) return (o) => o === id;
  let c = teamCache.get(s);
  if (!c || c.tick !== s.tick || c.n !== s.players.length) {
    const team = new Uint8Array(65536);
    for (const q of s.players) team[q.id] = q.team;
    c = { tick: s.tick, n: s.players.length, team };
    teamCache.set(s, c);
  }
  const t = c.team;
  const mine = p.team;
  // Allies are teammates unless the mode says otherwise; modes with custom allies take the slow, exact path.
  if (modeOf(s).allies) {
    const ids = homeIds(s, p);
    return (o) => ids.includes(o);
  }
  return (o) => o === id || (o !== 0 && t[o] === mine);
}

/**
 * Ticks a round lasts (0 = endless): the host's choice (cfg.roundTicks) for a mode that has rounds, else the
 * mode's own. @param {{cfg: import('../types.js').Config}} s
 */
export function roundLength(s) {
  const own = modeOf(s).roundTicks;
  return own > 0 && s.cfg.roundTicks > 0 ? s.cfg.roundTicks : own;
}

/** Whether the arena plays in rounds (with a results screen between them). @param {GameState} s */
export const hasRounds = (s) => modeOf(s).roundTicks > 0 || !!modeOf(s).roundOver;

/**
 * The results of a round: players ranked by score (teams: each team ranked by its members' total, members
 * listed under it). @param {GameState} s
 * @returns {import('../types.js').RoundResult}
 */
export function results(s) {
  const m = modeOf(s);
  const ranked = s.players.filter((p) => p.kind === 'human' || p.alive || p.land > 0);
  if (m.rank) ranked.sort((a, b) => /** @type {NonNullable<ModeDef['rank']>} */ (m.rank)(s, a, b) || a.id - b.id);
  else ranked.sort((a, b) => scoreOf(s, b) - scoreOf(s, a) || a.id - b.id);
  const players = ranked.map((p) => ({ id: p.id, name: p.name, bot: p.kind === 'bot', look: p.look, team: p.team, score: scoreOf(s, p), alive: p.alive }));
  if (m.teams) {
    /** @type {Map<number, number>} */
    const totals = new Map();
    for (const p of players) totals.set(p.team, (totals.get(p.team) ?? 0) + p.score);
    const order = [...totals.entries()].sort((a, b) => b[1] - a[1] || a[0] - b[0]);
    return { players, teams: order.map(([team, score]) => ({ team, score })), winner: m.winner ? m.winner(s) : order.length ? -order[0][0] : 0 };
  }
  return { players, teams: [], winner: m.winner ? m.winner(s) : (players[0]?.id ?? 0) };
}
