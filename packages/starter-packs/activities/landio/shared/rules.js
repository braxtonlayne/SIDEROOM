// @ts-check
// The Land.io simulation: `step(state, inputs)` advances one tick (a head moves one tile). Pure and
// deterministic: no clock, no Math.random, no I/O. It mutates the state it is given and returns it.

import { captureLoop } from './capture.js';
import { DX, DY, WALL, idx, inBounds, opposite, passable, playerById, setBase, setTrail } from './grid.js';
import { buildTerrain } from './maps/index.js';
import { TEAMS, allied, hasRounds, homeIds, modeOf, results, roundLength } from './modes/index.js';
import { LOOK_COLORS } from './look.js';
import { LOOKS } from './look.js';
import { rand, randInt, seedFrom } from './rng.js';
import { botThink, newBotMemory } from './bots.js';
import { cardAct, cardsTick, emptyHand, extraMoves, fxOn, hooks, offerCards, rankOf } from './cards.js';
import { registerKill } from './cards/util.js';

/** @typedef {import('./types.js').GameState} GameState */
/** @typedef {import('./types.js').Player} Player */
/** @typedef {import('./types.js').Config} Config */
/** @typedef {import('./types.js').Input} Input */

/** @type {Config} */
export const DEFAULT_CONFIG = {
  w: 100,
  h: 100,
  targetHeads: 12,
  tickHz: 7,
  botRespawnTicks: 14,
  spawnClearance: 5,
  cards: true,
  sandbox: false,
  spawnShieldTicks: 0,
  map: 'open',
  mode: 'classic',
  botSkill: 'normal',
  roundTicks: 0,
};

/** How long a round's results show before the next round starts (10 s). */
export const RESULTS_TICKS = 70;
/** In modes without respawns, people may still join this long after a round starts (10 s). */
export const JOIN_GRACE_TICKS = 70;


const BOT_NAMES = [
  'Ada', 'Bix', 'Cosmo', 'Dot', 'Echo', 'Fizz', 'Gizmo', 'Hex', 'Ion', 'Jinx', 'Kilo', 'Lumen', 'Mochi',
  'Nova', 'Orbit', 'Pixel', 'Quark', 'Rune', 'Sprocket', 'Tofu', 'Umbra', 'Vex', 'Widget', 'Yeti', 'Zed',
];

/**
 * A new arena. With `fill` the bot slots are filled at once.
 * @param {Partial<Config>} cfg @param {string|number} seed @param {{fill?: boolean}} [opt]
 * @returns {GameState}
 */
export function createState(cfg, seed, opt = {}) {
  const c = { ...DEFAULT_CONFIG, ...cfg };
  const n = c.w * c.h;
  /** @type {GameState} */
  const s = {
    cfg: c,
    tick: 0,
    rng: seedFrom(seed),
    base: new Uint16Array(n),
    trail: new Uint16Array(n),
    // The map is built from its own stream of the seed, so a map looks the same whatever the game does.
    terrain: buildTerrain(c.map, c.w, c.h, seed),
    terrainDirty: [],
    players: [],
    humans: 0,
    botCooldown: 0,
    events: [],
    dirtyMark: new Uint8Array(n),
    dirty: [],
    objects: [],
    nextObjectId: 0,
    seed: String(seed),
    round: { n: 1, phase: 'play', start: 0, end: roundLength({ cfg: c }), next: 0, result: null },
    md: {},
  };
  if (opt.fill) while (s.players.length < c.targetHeads && spawnBot(s)) { /* fill */ }
  modeOf(s).setup?.(s);
  return s;
}

/** Whether `p` (a human) may spawn now: always between rounds' starts in respawn modes; else only early in a
 * round, and only once per round. @param {GameState} s @param {Player | undefined} p */
export function maySpawn(s, p) {
  if (s.round.phase !== 'play') return false;
  if (modeOf(s).respawn) return true;
  if (p && p.death && p.death.tick >= s.round.start) return false;
  return s.tick - s.round.start <= JOIN_GRACE_TICKS;
}

/**
 * End the round: freeze everyone and keep the results for RESULTS_TICKS.
 * @param {GameState} s
 */
export function endRound(s) {
  if (s.round.phase === 'over') return;
  s.round.phase = 'over';
  s.round.result = results(s);
  s.round.next = s.tick + RESULTS_TICKS;
  s.events.push({ k: 'round', phase: 'over', n: s.round.n });
}

/**
 * Start a new round: a freshly built map (from the seed and the round number), nobody's land, every human who
 * was in the arena placed again (alive), the bots refilled, then the mode's setup.
 * @param {GameState} s
 */
export function startRound(s) {
  const { w, h } = s.cfg;
  const n = s.round.n + 1;
  // The host may have changed the arena's size: the tile layers are made anew.
  if (s.base.length !== w * h) {
    s.base = new Uint16Array(w * h);
    s.trail = new Uint16Array(w * h);
    s.dirtyMark = new Uint8Array(w * h);
  }
  s.terrain = buildTerrain(s.cfg.map, w, h, `${s.seed}|round:${n}`);
  s.terrainDirty = [];
  s.base.fill(0);
  s.trail.fill(0);
  s.dirtyMark.fill(0);
  s.dirty = [];
  s.objects = [];
  s.md = {};
  const humans = s.players.filter((p) => p.kind === 'human');
  s.players = [];
  s.round = { n, phase: 'play', start: s.tick, end: roundLength(s) ? s.tick + roundLength(s) : 0, next: 0, result: null };
  for (const p of humans) {
    const at = findSpawn(s) ?? findCarve(s);
    if (!at) continue;
    // The same player (id, name, uid) in a new life: the seat that owns the id finds it alive again.
    s.players.push(p);
    s.players.sort((a, b) => a.id - b.id);
    resetPlayer(s, p, at.x, at.y);
  }
  while (s.players.length < s.cfg.targetHeads && spawnBot(s)) { /* fill */ }
  s.botCooldown = 0;
  modeOf(s).setup?.(s);
  s.events.push({ k: 'round', phase: 'start', n });
}

/** @param {GameState} s */
function freeId(s) {
  let id = 1;
  for (const p of s.players) {
    if (p.id === id) id++;
    else break;
  }
  return id;
}

/** A look nobody else (but `self`) has. @param {GameState} s @param {Player} [self] */
function freeLook(s, self) {
  const used = new Set(s.players.filter((p) => p !== self).map((p) => p.look));
  for (let l = 0; l < LOOKS; l++) if (!used.has(l)) return l;
  return randInt(s, 0, LOOKS - 1);
}

/**
 * Whether a base centred on (cx, cy) may spawn: its 5×5 area is empty and no head or trail is within
 * `clear` tiles. @param {GameState} s @param {number} cx @param {number} cy @param {number} clear
 */
function spawnOk(s, cx, cy, clear) {
  const { w, h } = s.cfg;
  if (cx < 3 || cy < 3 || cx > w - 4 || cy > h - 4) return false;
  for (let y = cy - 2; y <= cy + 2; y++)
    for (let x = cx - 2; x <= cx + 2; x++) {
      const i = y * w + x;
      if (s.base[i] || s.trail[i] || s.terrain[i] === WALL) return false;
    }
  for (let y = Math.max(0, cy - clear); y <= Math.min(h - 1, cy + clear); y++)
    for (let x = Math.max(0, cx - clear); x <= Math.min(w - 1, cx + clear); x++)
      if (s.trail[y * w + x]) return false;
  for (const p of s.players)
    if (p.alive && Math.max(Math.abs(p.x - cx), Math.abs(p.y - cy)) <= clear) return false;
  return true;
}

/** A free base centre, or null when no free area is left (see findCarve). @param {GameState} s */
export function findSpawn(s) {
  const { w, h, spawnClearance } = s.cfg;
  for (let t = 0; t < 200; t++) {
    const cx = randInt(s, 3, w - 4);
    const cy = randInt(s, 3, h - 4);
    if (spawnOk(s, cx, cy, spawnClearance)) return { x: cx, y: cy };
  }
  // Crowded: scan every centre from a random start, then accept a smaller clearance.
  for (const clear of [spawnClearance, 2]) {
    const start = randInt(s, 0, w * h - 1);
    for (let k = 0; k < w * h; k++) {
      const i = (start + k) % (w * h);
      const cx = i % w;
      const cy = (i - cx) / w;
      if (spawnOk(s, cx, cy, clear)) return { x: cx, y: cy };
    }
  }
  return null;
}

/**
 * Spawn a player with a 3×3 base. Humans start stopped and wait for their first direction; bots start
 * moving. Returns null when there is no room.
 * @param {GameState} s @param {{kind: 'human'|'bot', name: string, uid?: string|null}} who
 * @returns {Player|null}
 */
export function spawnPlayer(s, who) {
  // Humans always get in (D-36): when no free area is left they take a base out of existing land. In Classic,
  // bots only spawn where there is room, so a full map runs with fewer bots instead of eating people's land.
  // In round-based modes bots carve too: a map one side has filled would otherwise empty out of bots.
  const at = findSpawn(s) ?? (who.kind === 'human' || hasRounds(s) ? findCarve(s) : null);
  return at ? spawnPlayerAt(s, who, at.x, at.y) : null;
}

/**
 * A base centre on occupied land, for when the map has no free area (land changes hands on kills and rarely
 * returns to neutral). The 3×3 must hold no trail and no head, and no owner may lose their last tiles there;
 * among those, prefer the most neutral ground in the 5×5 around it and the farthest from any head. Null only
 * when every spot is blocked.
 * @param {GameState} s
 */
export function findCarve(s) {
  const { w, h } = s.cfg;
  const heads = s.players.filter((p) => p.alive);
  /** @type {{x: number, y: number} | null} */
  let best = null;
  let bestScore = -Infinity;
  /** @type {Map<number, number>} */
  const take = new Map();
  for (let cy = 3; cy <= h - 4; cy++)
    for (let cx = 3; cx <= w - 4; cx++) {
      let near = Infinity;
      for (const p of heads) near = Math.min(near, Math.max(Math.abs(p.x - cx), Math.abs(p.y - cy)));
      if (near <= 2) continue;
      take.clear();
      let ok = true;
      for (let y = cy - 1; y <= cy + 1 && ok; y++)
        for (let x = cx - 1; x <= cx + 1; x++) {
          const i = y * w + x;
          if (s.trail[i] || s.terrain[i] === WALL) {
            ok = false;
            break;
          }
          const b = s.base[i];
          if (b) take.set(b, (take.get(b) ?? 0) + 1);
        }
      if (!ok) continue;
      for (const [owner, n] of take) {
        const p = playerById(s, owner);
        if (p && p.land - n < 1) ok = false;
      }
      if (!ok) continue;
      let neutral = 0;
      for (let y = cy - 2; y <= cy + 2; y++) for (let x = cx - 2; x <= cx + 2; x++) if (!s.base[y * w + x]) neutral++;
      const score = neutral * 4 + Math.min(near, 12);
      if (score > bestScore) {
        bestScore = score;
        best = { x: cx, y: cy };
      }
    }
  return best;
}

/**
 * Spawn a player with its base centred on (x, y), without checking the room (tests and fixtures).
 * @param {GameState} s @param {{kind: 'human'|'bot', name: string, uid?: string|null}} who @param {number} x @param {number} y
 * @returns {Player}
 */
export function spawnPlayerAt(s, who, x, y) {
  /** @type {Player} */
  const p = {
    id: freeId(s),
    kind: who.kind,
    name: who.name.slice(0, 24),
    uid: who.uid ?? null,
    look: 0,
    alive: true,
    x,
    y,
    dir: -1,
    stopped: true,
    queue: [],
    trail: [],
    land: 0,
    gold: 0,
    team: 0,
    maxLand: 0,
    kills: 0,
    captured: 0,
    spawnTick: s.tick,
    bbox: [],
    bot: who.kind === 'bot' ? newBotMemory(s) : null,
    death: null,
    cards: emptyHand(),
    fx: {},
    offers: [],
  };
  s.players.push(p);
  s.players.sort((a, b) => a.id - b.id);
  resetPlayer(s, p, x, y);
  return p;
}

/**
 * Place `p` alive at (x, y) with a fresh 3x3 base and a clean slate (a new player, or a human in a new round).
 * Its team (team modes) and look are chosen here. @param {GameState} s @param {Player} p @param {number} x @param {number} y
 */
function resetPlayer(s, p, x, y) {
  const m = modeOf(s);
  p.alive = true;
  p.x = x;
  p.y = y;
  p.dir = p.kind === 'bot' ? randInt(s, 0, 3) : -1;
  p.stopped = p.kind !== 'bot';
  p.queue = [];
  p.trail = [];
  p.land = 0;
  p.gold = 0;
  p.maxLand = 0;
  p.kills = 0;
  p.captured = 0;
  p.spawnTick = s.tick;
  p.bbox = [];
  p.death = null;
  p.cards = emptyHand();
  p.fx = s.cfg.spawnShieldTicks > 0 ? { spawn: s.tick + s.cfg.spawnShieldTicks } : {};
  p.offers = [];
  if (p.bot) p.bot = newBotMemory(s);
  if (m.teams) {
    p.team = 0;
    p.team = m.pickTeam ? m.pickTeam(s, p) : smallestTeam(s, m.teams);
    // Team colour; the pattern tells teammates apart.
    const mates = s.players.filter((q) => q !== p && q.team === p.team).length;
    p.look = TEAMS[(p.team - 1) % TEAMS.length].color + LOOK_COLORS * (mates % 4);
  } else {
    p.team = 0;
    p.look = freeLook(s, p);
  }
  for (let yy = y - 1; yy <= y + 1; yy++)
    for (let xx = x - 1; xx <= x + 1; xx++) setBase(s, idx(s, xx, yy), p.id);
  s.events.push({ k: 'spawn', id: p.id, name: p.name });
  m.onSpawn?.(s, p);
}

/** The team (1..n) with the fewest living members, the lowest number on a tie. @param {GameState} s @param {number} n */
function smallestTeam(s, n) {
  let best = 1;
  let bestCount = Infinity;
  for (let t = 1; t <= n; t++) {
    const c = s.players.filter((q) => q.team === t && q.alive).length;
    if (c < bestCount) {
      best = t;
      bestCount = c;
    }
  }
  return best;
}

/** @param {GameState} s */
function spawnBot(s) {
  const used = new Set(s.players.map((p) => p.name));
  const free = BOT_NAMES.filter((n) => !used.has(n));
  const pool = free.length ? free : BOT_NAMES;
  const name = pool[randInt(s, 0, pool.length - 1)];
  return spawnPlayer(s, { kind: 'bot', name });
}

/**
 * Kill `p`. Its trail returns to neutral. Its base goes to the killer when there is a living killer other
 * than `p` (D-32), except tiles under the killer's own trail (a tile never holds an owner's base and trail
 * at once), which go neutral; otherwise all of it goes neutral. Bots leave the arena; a dead human stays
 * (alive = false) so the summary can be read, until respawned or removed.
 * @param {GameState} s @param {Player} p @param {string} cause @param {Player|null} killer
 */
export function killPlayer(s, p, cause, killer) {
  if (!p.alive) return;
  // Spawn protection (D-45): a fresh player is not cut, crashed into or blown up yet.
  if (fxOn(p, 'spawn', s.tick) && (cause === 'trail' || cause === 'head' || cause === 'grenade')) return;
  // A card may save p (Shield, Second Wind): it then handles what happens instead.
  if (cause !== 'removed' && hooks.preventDeath(s, p, cause, killer)) return;
  const wasLeader = p.land > 0 && rankOf(s, p) === 1;
  p.death = {
    tick: s.tick,
    cause,
    killer: killer && killer !== p ? killer.name : null,
    land: p.land,
    maxLand: p.maxLand,
    kills: p.kills,
    captured: p.captured,
    ticksAlive: s.tick - p.spawnTick,
  };
  if (killer && killer !== p) killer.kills++;
  for (const i of p.trail) if (s.trail[i] === p.id) setTrail(s, i, 0);
  p.trail = [];
  const heir = killer && killer !== p && killer.alive ? killer : null;
  let inherited = 0;
  if (p.bbox.length) {
    const { w } = s.cfg;
    for (let y = p.bbox[1]; y <= p.bbox[3]; y++)
      for (let x = p.bbox[0]; x <= p.bbox[2]; x++) {
        const i = y * w + x;
        if (s.base[i] !== p.id) continue;
        if (heir && s.trail[i] !== heir.id) {
          setBase(s, i, heir.id);
          inherited++;
        } else setBase(s, i, 0);
      }
  }
  if (heir) heir.captured += inherited;
  // Cards are lost on death; a living killer drafts new ones (a bounty card for killing the leader), in
  // modes that hand out cards for kills.
  p.cards = emptyHand();
  p.fx = {};
  p.offers = [];
  if (heir) {
    if ((modeOf(s).cards ?? 'kills') === 'kills') offerCards(s, heir, wasLeader);
    hooks.kill(s, heir, p);
  }
  p.bbox = [];
  p.alive = false;
  p.queue = [];
  s.events.push({
    k: 'kill',
    id: killer && killer !== p ? killer.id : 0,
    name: killer && killer !== p ? killer.name : '',
    v: p.id,
    vn: p.name,
    c: cause,
    n: inherited,
  });
  modeOf(s).onKill?.(s, p, killer && killer !== p ? killer : null, cause);
  if (p.kind === 'bot' && !p.alive) s.players = s.players.filter((q) => q !== p);
}

registerKill(killPlayer);

/** Remove a player (a human leaving, a surplus bot). @param {GameState} s @param {number} id */
export function removePlayer(s, id) {
  const p = playerById(s, id);
  if (!p) return;
  if (p.alive) killPlayer(s, p, 'removed', null);
  s.players = s.players.filter((q) => q !== p);
  s.events.push({ k: 'leave', id, name: p.name });
}

/** Queue one tick's input. @param {GameState} s @param {Input} inp */
export function applyInput(s, inp) {
  const p = playerById(s, inp.id);
  if (!p || !p.alive || s.round.phase !== 'play') return;
  if (inp.turns)
    for (const t of inp.turns) {
      if (!(t === 0 || t === 1 || t === 2 || t === 3)) continue;
      // Buffer: the turn to take at the next tile, plus one more, so a quick "up, right" both land.
      if (p.queue.length >= 2) p.queue[1] = t;
      else p.queue.push(t);
    }
  if (inp.toggleStop && p.dir >= 0) p.stopped = !p.stopped;
  if (inp.cards) for (const code of inp.cards) cardAct(s, p, code);
}

/** Take the next buffered turn that is legal. @param {Player} p */
function takeTurn(p) {
  while (p.queue.length) {
    const t = /** @type {number} */ (p.queue.shift());
    if (p.dir < 0) {
      p.dir = t;
      p.stopped = false;
      return;
    }
    if (t === p.dir) {
      if (p.stopped) {
        p.stopped = false;
        return;
      }
      continue;
    }
    // No 180° turn while trailing (it would be suicide on your own trail).
    if (t === opposite(p.dir) && p.trail.length > 0) continue;
    p.dir = t;
    p.stopped = false;
    return;
  }
}

/**
 * Advance `ticks` ticks (default one). `inputs` apply at the start of the first tick.
 * @param {GameState} s @param {Input[]} [inputs] @param {number} [ticks]
 * @returns {GameState}
 */
export function step(s, inputs = [], ticks = 1) {
  for (let k = 0; k < ticks; k++) stepOnce(s, k === 0 ? inputs : []);
  return s;
}

/** @param {GameState} s @param {Input[]} inputs */
function stepOnce(s, inputs) {
  s.tick++;
  // Between rounds nobody moves; the next round starts when the results have shown long enough.
  if (s.round.phase === 'over') {
    if (s.tick >= s.round.next) startRound(s);
    return;
  }
  for (const inp of inputs) applyInput(s, inp);
  for (const p of s.players)
    if (p.alive && p.bot) {
      const d = botThink(s, p);
      if (d >= 0) p.queue = [d];
    }

  cardsTick(s);

  // Speed cards let a head move more than one tile in a tick: the tick runs in sub-steps of one tile each,
  // every sub-step with the same rules, so a fast head can never skip over a trail.
  /** @type {Map<Player, number>} */
  const moves = new Map();
  let most = 0;
  for (const p of s.players) {
    if (!p.alive) continue;
    const n = fxOn(p, 'stun', s.tick) ? 0 : 1 + extraMoves(p, s.tick);
    moves.set(p, n);
    if (n > most) most = n;
  }
  for (let sub = 0; sub < most; sub++) moveOnce(s, (p) => (moves.get(p) ?? 0) > sub);
  hooks.objects(s);
  const m = modeOf(s);
  m.tick?.(s);

  manageBots(s);
  if ((s.round.end && s.tick >= s.round.end) || m.roundOver?.(s)) endRound(s);
}

/**
 * One tile of movement for every living player for which `moving(p)` holds: choose tiles, trail hits,
 * head-to-head, deaths, then the survivors move, lay trail or capture.
 * @param {GameState} s @param {(p: Player) => boolean} moving
 */
function moveOnce(s, moving) {
  // 1. Where every moving head goes. Leaving the map or entering a wall tile kills; a Wall card object stops
  //    the head.
  /** @type {{p: Player, x: number, y: number, i: number}[]} */
  const movers = [];
  /** @type {Map<Player, {cause: string, killer: Player|null}>} */
  const doomed = new Map();
  const doom = (/** @type {Player} */ p, /** @type {string} */ cause, /** @type {Player|null} */ killer) => {
    if (!doomed.has(p)) doomed.set(p, { cause, killer });
  };
  for (const p of s.players) {
    if (!p.alive || !moving(p)) continue;
    takeTurn(p);
    if (p.stopped || p.dir < 0) continue;
    const x = p.x + DX[p.dir];
    const y = p.y + DY[p.dir];
    if (!inBounds(s, x, y)) {
      doom(p, 'border', null);
      continue;
    }
    if (!passable(s, x, y)) {
      doom(p, 'wall', null);
      continue;
    }
    if (s.objects.length && hooks.blocked(s, x, y)) {
      p.stopped = true;
      continue;
    }
    movers.push({ p, x, y, i: idx(s, x, y) });
  }

  // 2. Stepping on a trail (as it was before this step) kills its owner, yourself included, unless a card
  //    (Phase) makes the mover pass harmlessly.
  for (const m of movers) {
    const owner = s.trail[m.i];
    if (!owner) continue;
    const v = owner === m.p.id ? m.p : playerById(s, owner);
    if (!v || !v.alive) continue;
    // Immunity never covers another head's own tile: that is a normal cut (D-39).
    // A teammate's trail is harmless (the mover passes over it and leaves it as it is).
    if (v !== m.p && allied(s, m.p, v)) continue;
    const onHead = v !== m.p && v.trail[v.trail.length - 1] === m.i;
    if (!onHead && hooks.trailImmune(s, m.p, v)) continue;
    if (v === m.p) doom(m.p, 'self', null);
    else doom(v, 'trail', m.p);
  }

  // 3. Head to head: two or more heads arriving on one tile, outside their own bases, all die. A head on its
  //    own base is safe and the others pass through it.
  /** @type {Map<number, {p: Player, i: number}[]>} */
  const at = new Map();
  for (const m of movers) {
    const list = at.get(m.i);
    if (list) list.push(m);
    else at.set(m.i, [m]);
  }
  for (const list of at.values()) {
    if (list.length < 2) continue;
    // A head on home ground (its land, or a teammate's) is safe.
    const exposed = list.filter((m) => !homeIds(s, m.p).includes(s.base[m.i]));
    if (exposed.length < 2) continue;
    // Teammates meeting head on both live; anyone meeting a non-teammate dies.
    for (const m of exposed) {
      const foe = exposed.find((o) => o !== m && !allied(s, m.p, o.p));
      if (foe) doom(m.p, 'head', foe.p);
    }
  }

  // 4. Deaths, in id order. A player a card saved (Shield, Second Wind) does not also move this step.
  /** @type {Set<Player>} */
  const saved = new Set();
  for (const p of s.players.slice()) {
    const d = doomed.get(p);
    if (!d) continue;
    killPlayer(s, p, d.cause, d.killer);
    if (p.alive) saved.add(p);
  }

  // 5. The survivors move: a trail on foreign ground, a capture on reaching home.
  for (const m of movers) {
    const p = m.p;
    if (!p.alive || saved.has(p)) continue;
    p.x = m.x;
    p.y = m.y;
    // Home: our land, or a teammate's (team modes). No trail there; arriving with one closes the loop.
    const owner = s.base[m.i];
    if (owner === p.id || (owner && p.team && allied(s, p, /** @type {Player} */ (playerById(s, owner))))) {
      if (p.trail.length) capture(s, p);
      continue;
    }
    const prev = s.trail[m.i];
    if (prev === p.id) {
      // Crossed its own trail unharmed (Phase): the loop pinches off, the trail now ends here.
      const k = p.trail.indexOf(m.i);
      for (const j of p.trail.slice(k + 1)) setTrail(s, j, 0);
      p.trail = p.trail.slice(0, k + 1);
      continue;
    }
    if (prev) {
      const o = playerById(s, prev);
      // A teammate's trail stays; our trail just skips this tile.
      if (o && allied(s, p, o)) continue;
      // Crossed another trail unharmed (Phase): that trail is cut back to here, its owner lives.
      if (o) {
        const k = o.trail.indexOf(m.i);
        for (const j of o.trail.slice(0, k)) setTrail(s, j, 0);
        o.trail = o.trail.slice(k + 1);
      }
    }
    setTrail(s, m.i, p.id);
    p.trail.push(m.i);
  }
}

/** Close `p`'s loop; kill heads it encloses and players it leaves with no land. @param {GameState} s @param {Player} p */
function capture(s, p) {
  const home = homeIds(s, p);
  const allies = home.length > 1 ? s.players.filter((q) => q !== p && home.includes(q.id)) : [];
  const gained = captureLoop(s, p, (owner) => owner !== p.id && home.includes(owner), allies);
  p.captured += gained.length;
  s.events.push({ k: 'capture', id: p.id, n: gained.length });
  if (!gained.length) return;
  const set = new Set(gained);
  for (const q of s.players.slice()) {
    if (q === p || !q.alive || allied(s, p, q)) continue;
    // A head whose tile was just enclosed loses its invulnerability: it is caught.
    if (set.has(idx(s, q.x, q.y))) killPlayer(s, q, 'enclosed', p);
    else if (q.land === 0) killPlayer(s, q, 'landless', p);
  }
  if (p.alive) hooks.capture(s, p, gained);
}

/** Keep the bots at `targetHeads - humans`: one leaves or arrives per tick at most. @param {GameState} s */
function manageBots(s) {
  const quota = Math.max(0, s.cfg.targetHeads - s.humans);
  const bots = s.players.filter((p) => p.kind === 'bot');
  if (bots.length > quota) {
    // The bot with least land leaves first (its land returns to neutral).
    let worst = bots[0];
    for (const b of bots) if (b.land < worst.land) worst = b;
    removePlayer(s, worst.id);
  } else if (bots.length < quota && (modeOf(s).botsJoin?.(s) ?? true)) {
    if (s.botCooldown > 0) s.botCooldown--;
    else if (spawnBot(s)) s.botCooldown = s.cfg.botRespawnTicks;
    // No room: searching a full map is costly, so wait longer before looking again.
    else s.botCooldown = s.cfg.botRespawnTicks * 3;
  }
}

/** The events since the last call, and forget them. @param {GameState} s */
export function takeEvents(s) {
  const out = s.events;
  s.events = [];
  return out;
}

/** Land share of a player in percent. @param {GameState} s @param {Player} p */
export const landPct = (s, p) => (100 * p.land) / (s.cfg.w * s.cfg.h);

/** A 32-bit FNV-1a hash of everything that matters, for determinism checks. @param {GameState} s */
export function stateHash(s) {
  let h = 2166136261;
  const mix = (/** @type {number} */ v) => {
    h ^= v & 0xffff;
    h = Math.imul(h, 16777619);
    h ^= v >>> 16;
    h = Math.imul(h, 16777619);
  };
  mix(s.tick);
  mix(s.rng);
  for (let i = 0; i < s.base.length; i++) {
    mix(s.base[i] | (s.trail[i] << 16));
  }
  const text = JSON.stringify(s.players) + JSON.stringify(s.objects);
  for (let i = 0; i < text.length; i++) mix(text.charCodeAt(i));
  return (h >>> 0).toString(16).padStart(8, '0');
}

export { rand };
