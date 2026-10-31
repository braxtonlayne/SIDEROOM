// @ts-check
// Server-side bots: a small state machine with a personality (picked at spawn).
//   idle    in its base with no plan: plan a loop, sized to the danger and the land
//   expand  walk the loop: out, turn, side; a leg that meets a trail is dropped
//   home    shortest path back to its base (BFS, never across its own trail)
//   attack  an enemy trail tile it reaches before the owner gets home, by the widest margin
//   flee    an enemy head can reach one of our trail tiles before we get home: go home now
// Every move of a trailing head must keep a path home: a step that cuts the head off from its base is not
// taken while another step keeps one. Styles: builder loops and rarely attacks, hunter attacks often with
// smaller loops, balanced sits between. Every choice goes through the state's seeded RNG, so bots are
// deterministic.

import { extraMoves, fxOn } from './cards.js';
import { DX, DY, opposite, passable } from './grid.js';
import { allied, isHomeOwner, modeOf } from './modes/index.js';
import { rand, randInt } from './rng.js';

/** @typedef {import('./types.js').GameState} GameState */
/** @typedef {import('./types.js').Player} Player */
/** @typedef {'builder' | 'hunter' | 'balanced'} BotStyle */
/**
 * A bot's memory: the shared BotMemory (types.js) plus its style, picked at spawn and kept for the whole life.
 * @typedef {import('./types.js').BotMemory & {style: BotStyle}} BotState
 */

const STYLES = /** @type {BotStyle[]} */ (['builder', 'hunter', 'balanced']);
/** How much a mode's goal (botGoal) outweighs open room when a bot picks where to loop. */
const GOAL_PULL = 14;
/** Chance per tick of looking for an attack, and the loop size scale, by style. */
const TRAITS = /** @type {Record<BotStyle, {attack: number, loop: number}>} */ ({
  builder: { attack: 0.15, loop: 1.15 },
  hunter: { attack: 0.9, loop: 0.75 },
  balanced: { attack: 0.5, loop: 1 },
});

const ATTACK_RANGE = 6;
/**
 * By the host's bot skill: the attack chance scale, the chance per tick of noticing a threat to the trail, the
 * loop size scale and how far an attack may reach.
 */
const SKILL = /** @type {Record<string, {attack: number, notice: number, loop: number, range: number}>} */ ({
  easy: { attack: 0.15, notice: 0.35, loop: 1.3, range: 3 },
  normal: { attack: 1, notice: 1, loop: 1, range: ATTACK_RANGE },
  hard: { attack: 2.2, notice: 1, loop: 0.9, range: 9 },
});
/** @param {GameState} s */
const skillOf = (s) => SKILL[s.cfg.botSkill] ?? SKILL.normal;
/** An attack is given up when the trail it would make (so far plus the way to the target) is longer. */
const ATTACK_TRAIL_MAX = 20;
/** An enemy head this close (Manhattan) makes the bot plan small loops. */
const NEAR_ENEMY = 10;
/** Race safety: flee when an enemy reaches one of our trail tiles in fewer than home + RACE_MARGIN moves. */
const RACE_MARGIN = 1;
const BFS_LIMIT = 6000;

/** @param {GameState} s @returns {BotState} */
export function newBotMemory(s) {
  return {
    mode: 'idle',
    legs: [],
    target: -1,
    boldness: rand(s),
    style: STYLES[randInt(s, 0, STYLES.length - 1)],
  };
}

// BFS scratch: a visit stamp per tile avoids clearing between searches.
let stamp = new Uint32Array(0);
let firstDir = new Int8Array(0);
let distArr = new Int32Array(0);
let queue = new Int32Array(0);
let gen = 0;

/**
 * Breadth-first search from (x, y). Returns the first direction of a shortest path to a tile where
 * `goal(i)` holds, and its length; `{dir: -1}` when none within the node budget. Tiles where `block(i)`
 * holds are never entered.
 * @param {GameState} s @param {number} x @param {number} y
 * @param {(i: number) => boolean} goal @param {(i: number) => boolean} block @param {number} noDir a direction the first step may not take (-1 for none)
 */
export function bfs(s, x, y, goal, block, noDir) {
  const { w, h } = s.cfg;
  const n = w * h;
  if (stamp.length < n) {
    stamp = new Uint32Array(n);
    firstDir = new Int8Array(n);
    distArr = new Int32Array(n);
    queue = new Int32Array(n);
    gen = 0;
  }
  gen++;
  const start = y * w + x;
  stamp[start] = gen;
  let head = 0;
  let tail = 0;
  for (let d = 0; d < 4; d++) {
    if (d === noDir) continue;
    const nx = x + DX[d];
    const ny = y + DY[d];
    if (!passable(s, nx, ny)) continue;
    const i = ny * w + nx;
    if (stamp[i] === gen || block(i)) continue;
    stamp[i] = gen;
    firstDir[i] = d;
    distArr[i] = 1;
    if (goal(i)) return { dir: d, dist: 1 };
    queue[tail++] = i;
  }
  while (head < tail && tail < BFS_LIMIT) {
    const c = queue[head++];
    const cx = c % w;
    const cy = (c - cx) / w;
    for (let d = 0; d < 4; d++) {
      const nx = cx + DX[d];
      const ny = cy + DY[d];
      if (!passable(s, nx, ny)) continue;
      const i = ny * w + nx;
      if (stamp[i] === gen || block(i)) continue;
      stamp[i] = gen;
      firstDir[i] = firstDir[c];
      distArr[i] = distArr[c] + 1;
      if (goal(i)) return { dir: firstDir[i], dist: distArr[i] };
      queue[tail++] = i;
    }
  }
  return { dir: -1, dist: Infinity };
}

/** How many steps `p` needs to get home (Infinity when stuck). @param {GameState} s @param {Player} p */
function homeDistance(s, p) {
  if (!p.trail.length) return 0;
  const id = p.id;
  const home = isHomeOwner(s, p);
  return bfs(
    s,
    p.x,
    p.y,
    (i) => home(s.base[i]),
    (i) => s.trail[i] === id,
    p.dir >= 0 ? opposite(p.dir) : -1,
  ).dist;
}

/**
 * Steps from tile (x, y) to base `id` without crossing its trail; 0 on the base, Infinity when none. Used on a
 * candidate tile, so the head's own trail (which the move extends) is never entered.
 * @param {GameState} s @param {number} x @param {number} y @param {number} id
 */
function homeFrom(s, x, y, id) {
  const p = s.players.find((q) => q.id === id);
  const home = p ? isHomeOwner(s, p) : (/** @type {number} */ o) => o === id;
  if (home(s.base[y * s.cfg.w + x])) return 0;
  return bfs(
    s,
    x,
    y,
    (i) => home(s.base[i]),
    (i) => s.trail[i] === id,
    -1,
  ).dist;
}

/** Whether moving `p` in direction `d` is not immediately fatal to itself. @param {GameState} s @param {Player} p @param {number} d */
function safeDir(s, p, d) {
  if (p.trail.length && d === opposite(p.dir)) return false;
  const x = p.x + DX[d];
  const y = p.y + DY[d];
  if (!passable(s, x, y)) return false;
  return s.trail[y * s.cfg.w + x] !== p.id;
}

/** Free run in direction `d` before the border or a wall tile. @param {GameState} s @param {Player} p @param {number} d */
function room(s, p, d) {
  let n = 0;
  while (passable(s, p.x + DX[d] * (n + 1), p.y + DY[d] * (n + 1))) n++;
  return n;
}

/**
 * How many tiles without any trail lie in a row from (x, y) in direction `d`, counting up to `cap`.
 * @param {GameState} s @param {number} x @param {number} y @param {number} d @param {number} cap
 */
function freeRun(s, x, y, d, cap) {
  const { w, h } = s.cfg;
  let n = 0;
  let nx = x + DX[d];
  let ny = y + DY[d];
  while (n < cap && passable(s, nx, ny) && !s.trail[ny * w + nx]) {
    n++;
    nx += DX[d];
    ny += DY[d];
  }
  return n;
}

/** Manhattan distance from `p`'s head to tile index `i`. @param {GameState} s @param {Player} p @param {number} i */
function tileDist(s, p, i) {
  const x = i % s.cfg.w;
  const y = (i - x) / s.cfg.w;
  return Math.abs(x - p.x) + Math.abs(y - p.y);
}

/** Manhattan distance from `p` to the nearest living enemy head (Infinity when there is none). @param {GameState} s @param {Player} p */
function nearestEnemy(s, p) {
  let best = Infinity;
  for (const q of s.players) {
    if (q === p || !q.alive || allied(s, p, q)) continue;
    best = Math.min(best, Math.abs(q.x - p.x) + Math.abs(q.y - p.y));
  }
  return best;
}

/**
 * The race: can an enemy head reach one of our trail tiles in fewer moves than we need to get home (plus the
 * safety margin)? Manhattan distance is a lower bound on the moves, so this errs towards fleeing.
 * @param {GameState} s @param {Player} p @param {number} home our steps home from the head
 */
function threatened(s, p, home) {
  const w = s.cfg.w;
  const reach = home + RACE_MARGIN;
  for (const q of s.players) {
    if (q === p || !q.alive || allied(s, p, q)) continue;
    // Every trail tile is at most trail.length steps from the head, so a head further than that is out of reach.
    if (Math.abs(q.x - p.x) + Math.abs(q.y - p.y) >= reach + p.trail.length) continue;
    for (const i of p.trail) {
      const tx = i % w;
      const ty = (i - tx) / w;
      if (Math.abs(q.x - tx) + Math.abs(q.y - ty) < reach) return true;
    }
  }
  return false;
}

/**
 * Speed (Dash, Haste, Bloodlust) moves a head `sub` tiles in one tick, straight on, after the first move. Those
 * extra tiles must also stay on the grid and off our own trail, or the head dies on them.
 * @param {GameState} s @param {Player} p @param {number} d @param {number} sub
 */
function runOk(s, p, d, sub) {
  const { w, h } = s.cfg;
  for (let j = 2; j <= sub; j++) {
    const x = p.x + DX[d] * j;
    const y = p.y + DY[d] * j;
    if (!passable(s, x, y)) return false;
    if (s.trail[y * w + x] === p.id) return false;
  }
  return true;
}

/**
 * Whether a head stepping onto (x, y), off its own land, meets an enemy head's next tile: the two would land
 * on one tile or next to each other. @param {GameState} s @param {Player} p @param {number} x @param {number} y
 */
function headOnRisk(s, p, x, y) {
  const { w, h } = s.cfg;
  for (const q of s.players) {
    if (q === p || !q.alive || q.dir < 0 || allied(s, p, q)) continue;
    const ex = q.x + DX[q.dir];
    const ey = q.y + DY[q.dir];
    if (ex < 0 || ey < 0 || ex >= w || ey >= h) continue;
    if (Math.abs(ex - x) + Math.abs(ey - y) <= 1) return true;
  }
  return false;
}

/**
 * The enemy trail tile worth going for, or -1. A target is reached before its owner gets home; among those,
 * the widest margin (the owner's steps home minus ours) wins, then the nearer tile. Trails that would make ours
 * longer than ATTACK_TRAIL_MAX are not worth it.
 * @param {GameState} s @param {Player} p
 */
function attackTarget(s, p) {
  const { w, h } = s.cfg;
  let best = -1;
  let bestMargin = 0;
  let bestD = Infinity;
  /** @type {Map<number, number>} */
  const homeOf = new Map();
  const range = skillOf(s).range;
  for (let dy = -range; dy <= range; dy++) {
    const y = p.y + dy;
    if (y < 0 || y >= h) continue;
    const span = range - Math.abs(dy);
    for (let dx = -span; dx <= span; dx++) {
      const x = p.x + dx;
      if (x < 0 || x >= w) continue;
      const i = y * w + x;
      const o = s.trail[i];
      if (!o || o === p.id) continue;
      const owner = s.players.find((pl) => pl.id === o);
      if (owner && allied(s, p, owner)) continue;
      const d = Math.abs(dx) + Math.abs(dy);
      if (p.trail.length + d > ATTACK_TRAIL_MAX) continue;
      let home = homeOf.get(o);
      if (home === undefined) {
        const q = s.players.find((pl) => pl.id === o);
        home = q && q.alive ? homeDistance(s, q) : 0;
        homeOf.set(o, home);
      }
      // A stuck owner (Infinity) is capped, so its trail is still a target but does not outrank a reachable one by
      // an unbounded margin.
      const margin = Math.min(home, 200) - d;
      if (margin < 1) continue;
      if (margin > bestMargin || (margin === bestMargin && d < bestD)) {
        best = i;
        bestMargin = margin;
        bestD = d;
      }
    }
  }
  return best;
}

/**
 * Plan a loop out of the base. The loop is sized to the danger (small when an enemy head is within
 * NEAR_ENEMY, bigger when the area is clear) and to the land (a bot short of land goes bigger, one holding a lot
 * of it stays tighter). Legs stop short of any trail, so a planned loop never crosses one.
 * @param {GameState} s @param {Player} p @param {BotState} m
 */
function planLoop(s, p, m) {
  const { w, h } = s.cfg;
  const share = p.land / (w * h);
  const near = nearestEnemy(s, p);
  const scale =
    TRAITS[m.style].loop *
    skillOf(s).loop *
    (near <= NEAR_ENEMY ? 0.7 : near <= 20 ? 0.95 : 1.2) *
    (share < 0.03 ? 1.1 : share > 0.1 ? 0.9 : 1);
  const options = [0, 1, 2, 3].filter((d) => safeDir(s, p, d) && freeRun(s, p.x, p.y, d, 20) > 6);
  if (!options.length) return;
  // Prefer heading where there is open room; jitter so bots do not all agree. A mode may name a spot bots
  // should grow towards (a hill, the safe middle): directions that close in on it score higher.
  const goal = modeOf(s).botGoal?.(s, p) ?? null;
  let d1 = options[0];
  let bestScore = -1;
  for (const d of options) {
    const toward = goal ? Math.sign(goal.x - p.x) * DX[d] + Math.sign(goal.y - p.y) * DY[d] : 0;
    const score = freeRun(s, p.x, p.y, d, 20) + rand(s) * 10 + toward * GOAL_PULL;
    if (score > bestScore) {
      bestScore = score;
      d1 = d;
    }
  }
  const turn = rand(s) < 0.5 ? 1 : 3;
  const d2 = (d1 + turn) & 3;
  const outWant = Math.min(12, Math.max(2, Math.round(randInt(s, 3, 3 + Math.round(9 * m.boldness)) * scale)));
  const sideWant = Math.max(2, Math.round(randInt(s, 2, 4 + Math.round(6 * m.boldness)) * scale));
  const out = Math.min(outWant, freeRun(s, p.x, p.y, d1, 20) - 1);
  const cx = p.x + DX[d1] * out;
  const cy = p.y + DY[d1] * out;
  const side = Math.min(sideWant, freeRun(s, cx, cy, d2, 20) - 1);
  if (out < 2 || side < 2) return;
  m.legs = [d1, out, d2, side];
  m.mode = 'expand';
}

/**
 * The direction this bot wants this tick, or -1 to keep going.
 * @param {GameState} s @param {Player} p
 */
export function botThink(s, p) {
  const m = /** @type {BotState} */ (p.bot);
  const { w, h } = s.cfg;
  const trailing = p.trail.length > 0;
  if (!trailing && (m.mode === 'home' || m.mode === 'flee' || m.mode === 'attack')) {
    m.mode = 'idle';
    m.legs = [];
  }

  // Trailing heads: the steps home that each safe move leaves (Infinity: that move cuts us off from home).
  const homeD = [Infinity, Infinity, Infinity, Infinity];
  let home = Infinity;
  if (trailing) {
    for (let d = 0; d < 4; d++) if (safeDir(s, p, d)) homeD[d] = homeFrom(s, p.x + DX[d], p.y + DY[d], p.id);
    home = 1 + Math.min(...homeD);
  }

  // Danger is read by the race, not by a radius. Once fleeing, the bot stays on its way home.
  const skill = skillOf(s);
  if (trailing && m.mode !== 'flee' && home < Infinity && (skill.notice >= 1 || rand(s) < skill.notice) && threatened(s, p, home))
    m.mode = 'flee';

  if (m.mode !== 'flee' && rand(s) < TRAITS[m.style].attack * skill.attack) {
    const t = attackTarget(s, p);
    if (t >= 0) {
      m.mode = 'attack';
      m.target = t;
    }
  }
  if (m.mode === 'attack') {
    const t = m.target;
    const ok = t >= 0 && s.trail[t] !== 0 && s.trail[t] !== p.id && tileDist(s, p, t) + p.trail.length <= ATTACK_TRAIL_MAX;
    if (!ok) m.mode = trailing ? 'home' : 'idle';
  }
  if (m.mode === 'idle') planLoop(s, p, m);

  let want = -1;
  if (m.mode === 'expand') {
    if (!m.legs.length) m.mode = trailing ? 'home' : 'idle';
    else {
      want = m.legs[0];
      const nx = p.x + DX[want];
      const ny = p.y + DY[want];
      // A trail ahead (anyone's) ends the loop: the bot goes home from where it is.
      if (!passable(s, nx, ny) || s.trail[ny * w + nx]) {
        m.mode = trailing ? 'home' : 'idle';
        m.legs = [];
        want = -1;
      } else if (trailing || s.base[ny * w + nx] !== p.id) {
        // Legs count tiles walked outside the base; inside it the bot just heads out.
        m.legs[1]--;
        if (m.legs[1] <= 0) m.legs.splice(0, 2);
      }
    }
  }
  if (m.mode === 'attack') {
    const target = m.target;
    want = bfs(s, p.x, p.y, (i) => i === target, (i) => s.trail[i] === p.id, trailing ? opposite(p.dir) : -1).dir;
    if (want < 0) m.mode = trailing ? 'home' : 'idle';
  }
  if (m.mode === 'home' || m.mode === 'flee') {
    const home = isHomeOwner(s, p);
    want = bfs(s, p.x, p.y, (i) => home(s.base[i]), (i) => s.trail[i] === p.id, opposite(p.dir)).dir;
  }
  if (want < 0) want = p.dir;

  // Pick among the safe moves. Ranking, best first: a move that keeps a path home (trailing only); off our land,
  // one that does not meet an enemy head's next tile; the planned direction; the fewest steps home; the most room.
  // This tick's moves: one, plus the speed effects that will apply (a Dash the bot may start in this tick's card
  // step is counted too, since cards run after the bot decides).
  const dashMayStart = p.cards.active?.id === 'dash' && s.tick >= p.cards.active.ready && !fxOn(p, 'dash', s.tick);
  const sub = 1 + extraMoves(p, s.tick) + (dashMayStart ? 1 : 0);
  let best = -1;
  let bestKey = Infinity;
  for (let d = 0; d < 4; d++) {
    if (!safeDir(s, p, d) || !runOk(s, p, d, sub)) continue;
    const x = p.x + DX[d];
    const y = p.y + DY[d];
    const hd = trailing ? homeD[d] : 0;
    const cls = (hd === Infinity ? 4 : 0) + (s.base[y * w + x] !== p.id && headOnRisk(s, p, x, y) ? 2 : 0);
    const key = cls * 1e8 + (d === want ? 0 : 1e7) + Math.min(hd, 9999) * 100 + (99 - Math.min(room(s, p, d), 99));
    if (key < bestKey) {
      bestKey = key;
      best = d;
    }
  }
  return best;
}
