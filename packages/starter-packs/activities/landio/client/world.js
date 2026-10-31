// @ts-check
// The client's copy of the arena, rebuilt only from what the server sends. Tick records are played out on a
// local clock a little behind the newest tick received, so every head moves smoothly between tile centres
// and tiles change exactly when the head that changed them gets there.
//
// Playout clock: r(now) = now / tickMs - offset. A record for tick t must be here before r reaches t - 1
// (interpolating towards t needs t). Each arrival gives a sample now / tickMs - t; the offset follows the
// worst recent sample + 1 + a margin, slewing slowly so motion never jumps.

import { rleDecode, FLAG_STOPPED, FLAG_TRAILING } from '../shared/codec.js';
import { REGISTRY, MAX_EXTRA_MOVES } from '../shared/cards.js';

/** @typedef {import('./net/transport.js').PollReply} PollReply */
/** @typedef {{x: number, y: number, dir: number, flags: number}} Head */
/** @typedef {{t: number, tiles: number[], heads: number[], objs?: number[], ter?: number[], ev: any[]}} Rec */
/** @typedef {{id: number, kind: string, x: number, y: number, dir: number, until: number, owner: number}} Obj */

const SAMPLE_WINDOW_MS = 3000;
/** Tiles per side of a render chunk: the renderer caches each chunk's pixels until its version changes. */
export const CHUNK = 16;
/** More changed tiles than this in one chunk and it is simply repainted whole. */
const DIRTY_MAX = 64;
const MARGIN = 0.2;
const HEAD_HISTORY = 40;

export class World {
  constructor() {
    this.w = 0;
    this.h = 0;
    this.base = new Uint16Array(0);
    this.trail = new Uint16Array(0);
    /** Per tile: 0 open, 1 wall, 2 gold (shared/grid.js). */
    this.terrain = new Uint8Array(0);
    /** The map id of the arena. */
    this.map = 'open';
    /** Chunks per row, and a version per chunk bumped whenever a tile in it (or on its border) changes. */
    this.cw = 0;
    this.chunkVer = new Uint32Array(0);
    /** Bumped whenever any tile changes (the minimap redraws only then). */
    this.tilesVer = 0;
    /**
     * Changed tiles per chunk since the renderer last took them ('all' = repaint it whole), so a repaint can be
     * clipped to the tiles that changed and their neighbours (they share the border lines).
     * @type {Map<number, Set<number> | 'all'>}
     */
    this.dirtyTiles = new Map();
    /** Bumped when everything changed at once (a snapshot, a new roster): every chunk repaints whole. */
    this.allGen = 0;
    this.tickMs = 1000 / 7;
    /** Newest tick received, the newest tick with every tick before it received, and the newest applied. */
    this.recvTick = 0;
    this.contig = 0;
    this.appliedTick = 0;
    /** @type {Map<number, Rec>} */
    this.queue = new Map();
    /** @type {Map<number, Map<number, Head>>} heads at each recent tick */
    this.heads = new Map();
    /** @type {Map<number, Obj[]>} card objects at each recent tick */
    this.objs = new Map();
    /** @type {Map<number, {name: string, bot: boolean, look: number, team: number}>} */
    this.roster = new Map();
    /** The round, mode and map (see shared/codec.js encodeRound). @type {NonNullable<PollReply['round']> | null} */
    this.round = null;
    /** The arena's settings and host (the settings menu). @type {NonNullable<PollReply['game']> | null} */
    this.game = null;
    /**
     * Versions of the round and settings held, and the server tick each part (and the stats) came with: replies
     * can arrive out of order, so an older one never overwrites a newer one.
     */
    this.gv = 0;
    this.rdv = 0;
    this.gameAt = -1;
    this.roundAt = -1;
    this.statsAt = -1;
    this.rv = 0;
    /** @type {Map<number, {land: number, kills: number, alive: boolean, score: number}>} */
    this.stats = new Map();
    /** @type {PollReply['you'] | null} */
    this.you = null;
    this.offset = NaN;
    /** @type {{at: number, v: number}[]} */
    this.samples = [];
    this.lastFrame = 0;
    this.underruns = 0;
    /** Called as records are played: tiles that turned into someone's base, and events. */
    /** @type {(owner: number, tiles: number[], head: Head | undefined) => void} */
    this.onCapture = () => {};
    /** @type {(e: any) => void} */
    this.onEvent = () => {};
    /** @type {() => void} */
    this.onFull = () => {};
  }

  /**
   * Whether tile i is home to player `me`: their land, or a teammate's (team modes: no trail there, and arriving
   * closes a loop). @param {number} i @param {number} me
   */
  isHome(i, me) {
    const b = this.base[i];
    if (b === me) return true;
    if (!b) return false;
    const team = this.roster.get(me)?.team ?? 0;
    return team > 0 && this.roster.get(b)?.team === team;
  }

  /** Forget everything about this arena: the next reply (a snapshot of another room) starts the world over. */
  reset() {
    this.w = 0;
    this.h = 0;
    this.recvTick = 0;
    this.contig = 0;
    this.appliedTick = 0;
    this.queue.clear();
    this.heads.clear();
    this.objs.clear();
    this.roster.clear();
    this.rv = 0;
    this.stats.clear();
    this.you = null;
    this.round = null;
    this.game = null;
    this.gv = 0;
    this.rdv = 0;
    this.gameAt = this.roundAt = this.statsAt = -1;
    this.offset = NaN;
    this.samples = [];
    this.dirtyTiles.clear();
  }

  /** The cursor for the next poll. */
  cursor() {
    return { since: this.contig, rv: this.rv, gv: this.gv, rdv: this.rdv, newest: this.recvTick, tickMs: this.tickMs };
  }

  /** Take in a poll reply. @param {PollReply} r @param {number} at arrival time (performance.now) */
  ingest(r, at) {
    if (!r.ok) return;
    if (r.hz) this.tickMs = 1000 / r.hz;
    if (r.full && (r.full.t >= this.contig || !this.w)) {
      const f = r.full;
      // A snapshot is a fresh start (a new arena or server too): take whatever comes with it.
      this.gameAt = this.roundAt = this.statsAt = -1;
      if (f.w !== this.w || f.h !== this.h) {
        this.w = f.w;
        this.h = f.h;
        this.base = new Uint16Array(f.w * f.h);
        this.trail = new Uint16Array(f.w * f.h);
        this.terrain = new Uint8Array(f.w * f.h);
        this.cw = Math.ceil(f.w / CHUNK);
        this.chunkVer = new Uint32Array(this.cw * Math.ceil(f.h / CHUNK));
      }
      this.touchAll();
      rleDecode(f.base, this.base);
      rleDecode(f.trail, this.trail);
      if (f.terrain) rleDecode(f.terrain, this.terrain);
      else this.terrain.fill(0);
      this.map = f.map ?? 'open';
      // Records already here for later ticks stay; older ones are covered by the snapshot.
      for (const t of [...this.queue.keys()]) if (t <= f.t) this.queue.delete(t);
      for (const t of [...this.heads.keys()]) if (t < f.t) this.heads.delete(t);
      this.heads.set(f.t, decodeHeads(f.heads));
      this.objs.set(f.t, decodeObjs(f.objs ?? []));
      this.recvTick = Math.max(this.recvTick, f.t);
      this.contig = f.t;
      this.appliedTick = f.t;
      this.samples = [];
      this.offset = NaN;
      this.addSample(at, f.t);
      this.onFull();
    }
    if (r.recs)
      for (const rec of r.recs) {
        // Polls overlap and can arrive out of order: keep each record once.
        if (rec.t <= this.contig || this.queue.has(rec.t)) continue;
        this.queue.set(rec.t, rec);
        this.heads.set(rec.t, decodeHeads(rec.heads));
        this.objs.set(rec.t, decodeObjs(rec.objs ?? []));
        if (rec.t > this.recvTick) this.recvTick = rec.t;
        this.addSample(at, rec.t);
      }
    // A gap is filled by the next poll, which asks from `contig` (or gets a snapshot if it is too old).
    while (this.queue.has(this.contig + 1)) this.contig++;
    for (const t of this.heads.keys()) if (t < this.recvTick - HEAD_HISTORY) this.heads.delete(t);
    for (const t of this.objs.keys()) if (t < this.recvTick - HEAD_HISTORY) this.objs.delete(t);
    if (r.roster) {
      const next = new Map(r.roster.map((p) => [p.id, { name: p.name, bot: p.bot, look: p.look, team: p.team ?? 0 }]));
      // Tiles are drawn by owner id: only an id that now has another look needs everything repainted (ids are
      // reused). Players arriving or leaving change no pixels by themselves.
      let recolour = false;
      for (const [id, p] of next) {
        const old = this.roster.get(id);
        if (old && old.look !== p.look) recolour = true;
      }
      this.roster = next;
      if (recolour) this.touchAll();
      this.rv = r.rv ?? this.rv;
    }
    if (r.stats && r.now >= this.statsAt) {
      this.statsAt = r.now;
      this.stats.clear();
      for (let k = 0; k < r.stats.length; k += 5)
        this.stats.set(r.stats[k], { land: r.stats[k + 1], kills: r.stats[k + 2], alive: !!r.stats[k + 3], score: r.stats[k + 4] });
    }
    if (r.round && r.now >= this.roundAt) {
      this.round = r.round;
      this.roundAt = r.now;
      this.rdv = r.rdv ?? 0;
    }
    if (r.game && r.now >= this.gameAt) {
      this.game = r.game;
      this.gameAt = r.now;
      this.gv = r.gv ?? 0;
    }
    if (r.you) this.you = r.you;
  }

  /** Mark the chunk of tile i stale, and its neighbours' when i is on a chunk border (they draw edges). @param {number} i */
  touch(i) {
    const x = i % this.w;
    const y = (i - x) / this.w;
    // The tile and its 4 neighbours: a border line between two owners is drawn on both sides of it.
    this.dirtyTile(x, y);
    if (x > 0) this.dirtyTile(x - 1, y);
    if (x + 1 < this.w) this.dirtyTile(x + 1, y);
    if (y > 0) this.dirtyTile(x, y - 1);
    if (y + 1 < this.h) this.dirtyTile(x, y + 1);
  }

  /** @param {number} x @param {number} y */
  dirtyTile(x, y) {
    const key = Math.floor(y / CHUNK) * this.cw + Math.floor(x / CHUNK);
    this.chunkVer[key]++;
    const set = this.dirtyTiles.get(key);
    if (set === 'all') return;
    if (!set) this.dirtyTiles.set(key, new Set([y * this.w + x]));
    else if (set.size >= DIRTY_MAX) this.dirtyTiles.set(key, 'all');
    else set.add(y * this.w + x);
  }

  /** The changed tiles of a chunk since the last call ('all', or undefined for none), and forget them. @param {number} key */
  takeDirtyTiles(key) {
    const d = this.dirtyTiles.get(key);
    this.dirtyTiles.delete(key);
    return d;
  }

  touchAll() {
    this.tilesVer++;
    this.allGen++;
    this.dirtyTiles.clear();
    for (let k = 0; k < this.chunkVer.length; k++) this.chunkVer[k]++;
  }

  /** @param {number} at @param {number} t */
  addSample(at, t) {
    this.samples.push({ at, v: at / this.tickMs - t });
    while (this.samples.length && this.samples[0].at < at - SAMPLE_WINDOW_MS) this.samples.shift();
  }

  /** The playout tick for this frame (fractional). @param {number} now */
  playout(now) {
    if (!this.samples.length) return this.appliedTick;
    let worst = -Infinity;
    for (const s of this.samples) if (s.v > worst) worst = s.v;
    const target = worst + 1 + MARGIN;
    const dt = this.lastFrame ? Math.min(now - this.lastFrame, 100) : 0;
    this.lastFrame = now;
    if (!Number.isFinite(this.offset) || Math.abs(target - this.offset) > 6) this.offset = target;
    else {
      // Slew at most 10% of real time, so the game speeds up or slows down a little instead of jumping.
      const max = (0.1 * dt) / this.tickMs;
      this.offset += Math.max(-max, Math.min(max, target - this.offset));
    }
    let r = now / this.tickMs - this.offset;
    // Never play past what we have without gaps: hold there (an underrun).
    if (r > this.contig) {
      r = this.contig;
      this.underruns++;
    }
    return r;
  }

  /** Apply every queued record up to tick `upTo`. @param {number} upTo */
  advance(upTo) {
    while (this.appliedTick < upTo) {
      const t = this.appliedTick + 1;
      const rec = this.queue.get(t);
      if (!rec) break;
      this.queue.delete(t);
      this.applyRec(rec);
      this.appliedTick = t;
    }
  }

  /** @param {Rec} rec */
  applyRec(rec) {
    const heads = this.heads.get(rec.t);
    /** @type {Map<number, number[]>} */
    const captured = new Map();
    const tl = rec.tiles;
    for (let k = 0; k < tl.length; k += 3) {
      const i = tl[k];
      const b = tl[k + 1];
      if (b && this.base[i] !== b) {
        const list = captured.get(b);
        if (list) list.push(i);
        else captured.set(b, [i]);
      }
      this.base[i] = b;
      this.trail[i] = tl[k + 2];
      this.touch(i);
      this.tilesVer++;
    }
    if (rec.ter)
      for (let k = 0; k < rec.ter.length; k += 2) {
        this.terrain[rec.ter[k]] = rec.ter[k + 1];
        this.touch(rec.ter[k]);
        this.tilesVer++;
      }
    for (const [owner, tiles] of captured) this.onCapture(owner, tiles, heads?.get(owner));
    for (const e of rec.ev) this.onEvent(e);
  }

  /**
   * Where a head is drawn at playout tick r: between its tiles at floor(r) and floor(r)+1.
   * @param {number} id @param {number} r @returns {{x: number, y: number, dir: number, flags: number} | null}
   */
  headAt(id, r) {
    const k = Math.floor(r);
    const a = this.heads.get(k)?.get(id);
    const b = this.heads.get(k + 1)?.get(id);
    if (!a) return b ? { ...b } : null;
    if (!b) return { ...a };
    const f = r - k;
    // A head that teleports (a respawn, Rewind, Second Wind) does not slide across the map; one that moved
    // 2-3 tiles in a tick (Dash, Haste) is just fast and slides.
    if (Math.abs(b.x - a.x) + Math.abs(b.y - a.y) > 1 + MAX_EXTRA_MOVES) return { ...(f < 0.5 ? a : b) };
    return { x: a.x + (b.x - a.x) * f, y: a.y + (b.y - a.y) * f, dir: b.dir, flags: b.flags };
  }

  /** Ids of heads alive at playout tick r. @param {number} r */
  headIds(r) {
    const k = Math.floor(r);
    const ids = new Set(this.heads.get(k)?.keys() ?? []);
    for (const id of this.heads.get(k + 1)?.keys() ?? []) ids.add(id);
    return ids;
  }
}

/** @param {number[]} a @returns {Obj[]} */
function decodeObjs(a) {
  /** @type {Obj[]} */
  const out = [];
  for (let k = 0; k < a.length; k += 7)
    out.push({ id: a[k], kind: REGISTRY[a[k + 1]]?.id ?? '?', x: a[k + 2], y: a[k + 3], dir: a[k + 4], until: a[k + 5], owner: a[k + 6] });
  return out;
}

/** @param {number[]} a @returns {Map<number, Head>} */
function decodeHeads(a) {
  /** @type {Map<number, Head>} */
  const m = new Map();
  for (let k = 0; k < a.length; k += 5) m.set(a[k], { x: a[k + 1], y: a[k + 2], dir: a[k + 3], flags: a[k + 4] });
  return m;
}

export { FLAG_STOPPED, FLAG_TRAILING };
