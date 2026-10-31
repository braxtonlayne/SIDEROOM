// @ts-check
// Own-head prediction. Everyone else is drawn on the playout clock (behind the server); your own head is
// drawn on a clock *ahead* of the server by the time an input needs to reach it, re-simulated every frame
// from the newest server state of your head plus the actions the server has not applied yet. Each action
// is sent with the tick it should apply on (the tile you saw your head reach), so the server turns you
// exactly there whenever it arrives in time; if it arrives late, the next server state corrects the head.
// Only your own movement is predicted: no captures, no kills.

import { DX, DY, WALL } from '../shared/grid.js';
import { ACT, CARDS, extraMoves } from '../shared/cards.js';
import { FLAG_STOPPED } from './world.js';

/** @typedef {import('./world.js').World} World */
/** @typedef {{x: number, y: number, dir: number, stopped: boolean}} PHead */

const WINDOW_MS = 3000;
/** Actions that may wait for tiles ahead (the turn for the next tile plus one more). */
const MAX_AHEAD = 2;
/** Ticks of slack on top of the one-way trip, for jitter. */
const MARGIN = 0.35;

export class Predictor {
  /** @param {World} world */
  constructor(world) {
    this.world = world;
    /** @type {{seq: number, d: number, at: number}[]} actions not yet applied by the server */
    this.acts = [];
    /** @type {{at: number, v: number}[]} downlink samples, in ticks */
    this.samples = [];
    this.off = NaN;
    this.lastFrame = 0;
  }

  /** Another room: its clock and inputs have nothing to do with the last one's. */
  reset() {
    this.acts = [];
    this.samples = [];
    this.off = NaN;
    this.lastFrame = 0;
  }

  /** Learn the server clock from a poll reply. @param {{now: number, ms: number}} r @param {number} at */
  onReply(r, at) {
    const tm = this.world.tickMs;
    this.samples.push({ at, v: at / tm - (r.now + r.ms / tm) });
    while (this.samples.length && this.samples[0].at < at - WINDOW_MS) this.samples.shift();
  }

  /** The server's tick now, fractional (the fastest recent reply bounds the downlink delay). @param {number} now */
  serverNow(now) {
    let min = Infinity;
    for (const s of this.samples) if (s.v < min) min = s.v;
    return Number.isFinite(min) ? now / this.world.tickMs - min : this.world.recvTick;
  }

  /**
   * The predicted clock for this frame: server now + one-way uplink + margin, slewed so it never jumps.
   * @param {number} now @param {number} rttMs @param {number} floor never show the own head behind this
   */
  clock(now, rttMs, floor) {
    const tm = this.world.tickMs;
    const off = now / tm - (this.serverNow(now) + rttMs / 2 / tm + MARGIN);
    const dt = this.lastFrame ? Math.min(now - this.lastFrame, 100) : 0;
    this.lastFrame = now;
    if (!Number.isFinite(this.off) || Math.abs(off - this.off) > 4) this.off = off;
    else {
      const max = (0.1 * dt) / tm;
      this.off += Math.max(-max, Math.min(max, off - this.off));
    }
    return Math.max(now / tm - this.off, floor);
  }

  /**
   * Forget actions the server has applied. The ack says "up to seq N, as of tick T", but actions apply in
   * TICK order, not send order: a card action sent after a turn can apply before it. So an action is only
   * known to be applied when its seq is acked AND its tick has come (a.at <= T).
   * @param {number} ackSeq @param {number} ackTick
   */
  prune(ackSeq, ackTick) {
    this.acts = this.acts.filter((a) => a.seq > ackSeq || a.at > ackTick);
  }

  /**
   * Plan a new action (a direction, or -1 for stop/go) at clock P. A turn takes effect on leaving the tile
   * the head is heading into (or the one it just reached). Like a snake game's input buffer, at most
   * MAX_AHEAD actions wait for tiles ahead, one per tick; another press replaces the last waiting one
   * (the server swaps it too: same tick). A press that would change nothing — the direction the head will
   * already have, or a reversal on a trail — is dropped. Returns null to drop the press.
   * @param {number} me @param {number} d @param {number} P @param {boolean} moving
   * @returns {{at: number, replaces: number} | null}  replaces = seq of the action it replaces, or 0
   */
  plan(me, d, P, moving) {
    const k = Math.floor(P);
    // Card actions apply on the next tick and do not take a turn slot.
    if (d < -1) return { at: k + 1, replaces: 0 };
    const waiting = this.acts.filter((a) => a.at > k && a.d >= -1).sort((a, b) => a.at - b.at);
    let at = !moving || P - k < 0.2 ? k + 1 : k + 2;
    let replaces = 0;
    if (waiting.length >= MAX_AHEAD) {
      const last = /** @type {{seq: number, d: number, at: number}} */ (waiting.at(-1));
      at = last.at;
      replaces = last.seq;
    } else if (waiting.length) at = Math.max(at, /** @type {{at: number}} */ (waiting.at(-1)).at + 1);
    if (d >= 0) {
      // The head as it will be when this action applies, without the action it replaces.
      const others = this.acts.filter((a) => a.seq !== replaces);
      const before = this.path(me, at - 1, others)?.get(at - 1);
      if (before && before.dir >= 0) {
        const trailing = !this.world.isHome(before.y * this.world.w + before.x, me);
        if (d === before.dir && !before.stopped) return null;
        if (d === ((before.dir + 2) & 3) && trailing) return null;
      }
    }
    return { at, replaces };
  }

  /** @param {number} seq @param {number} d @param {number} at @param {number} [replaces] */
  add(seq, d, at, replaces = 0) {
    if (replaces) this.acts = this.acts.filter((a) => a.seq !== replaces);
    this.acts.push({ seq, d, at });
  }

  /**
   * Own head positions from the newest server tick up to `upTo`, as a map tick -> head. Null when the head is
   * not in the newest server state (dead, not spawned).
   * @param {number} me @param {number} upTo @param {{seq: number, d: number, at: number}[]} [pending]
   * @returns {Map<number, PHead> | null}
   */
  path(me, upTo, pending = this.acts) {
    const w = this.world;
    const K = w.recvTick;
    const h0 = w.heads.get(K)?.get(me);
    if (!h0) return null;
    /** @type {Map<number, PHead>} */
    const out = new Map();
    let p = { x: h0.x, y: h0.y, dir: h0.dir, stopped: !!(h0.flags & FLAG_STOPPED) };
    out.set(K, p);
    const acts = [...pending].sort((a, b) => a.at - b.at || a.seq - b.seq);
    // Own cards and timers as the server last told us, for speed (Dash, Haste) and stun.
    const hand = w.you?.hand;
    const self = {
      cards: {
        items: [],
        active: hand?.active ? { id: hand.active[0], level: hand.active[1], ready: hand.active[2] } : null,
        passives: (hand?.passives ?? []).map(([id, level]) => ({ id, level })),
      },
      fx: { ...(hand?.fx ?? {}) },
    };
    const objs = w.objs.get(K) ?? [];
    for (let t = K + 1; t <= upTo; t++) {
      p = { ...p };
      const trailing = !w.isHome(p.y * w.w + p.x, me);
      for (const a of acts) {
        // An action whose tick has passed without being applied lands on the next server tick.
        if (Math.max(a.at, K + 1) !== t) continue;
        if (a.d === ACT.active) {
          const act = self.cards.active;
          const def = act && CARDS[act.id];
          if (act && def && act.ready <= t && !((self.fx.emp ?? 0) > t)) {
            def.predictUse?.(act.level, self.fx, t);
            act.ready = t + (def.cooldown ? def.cooldown(act.level) : 0);
          }
          continue;
        }
        if (a.d < -1) continue; // other card actions do not move the head
        if (a.d === -1) {
          if (p.dir >= 0) p.stopped = !p.stopped;
        } else if (p.dir < 0) {
          p.dir = a.d;
          p.stopped = false;
        } else if (a.d === p.dir) p.stopped = false;
        else if (!(a.d === ((p.dir + 2) & 3) && trailing)) {
          p.dir = a.d;
          p.stopped = false;
        }
      }
      const steps = (self.fx.stun ?? 0) > t ? 0 : 1 + extraMoves(/** @type {any} */ (self), t);
      let hitWall = false;
      for (let k = 0; k < steps && !p.stopped && p.dir >= 0; k++) {
        const nx = p.x + DX[p.dir];
        const ny = p.y + DY[p.dir];
        if (nx < 0 || ny < 0 || nx >= w.w || ny >= w.h || w.terrain[ny * w.w + nx] === WALL) {
          hitWall = true;
          break; // the server will kill us at the wall; hold
        }
        // A card object that blocks (a Wall) stops the head, as the server will.
        if (objs.some((o) => CARDS[o.kind]?.objectBlocks?.(/** @type {any} */ ({ tick: t }), /** @type {any} */ (o), nx, ny))) {
          p.stopped = true;
          break;
        }
        p.x = nx;
        p.y = ny;
      }
      out.set(t, p);
      if (hitWall) break;
    }
    return out;
  }
}
