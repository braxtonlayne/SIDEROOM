// @ts-check
// The host side of Land.io as a SIDE ROOM activity. In the standalone game this was server/api.js plus
// server/rooms.js on a node:http server; here the same authoritative arena (./arena.js, unchanged logic) runs in
// the browser of the member who launched the activity, and members reach it through the Embedded App SDK's data
// channel (ACT-CH-001) instead of HTTP.
//
// The engine knows nothing about the SDK. It is given `send(memberId, message)` (an addressed reply) and
// `publish(frame)` (one broadcast every tick) and is fed messages with `handle(memberId, name, message)`:
//
//   guest -> host     { t: 'hello', name? }              sit down; answered by 'welcome' (a full snapshot)
//                     { t: 'in', acts: [[seq, dir, at?], ...] }   inputs, exactly as POST /input carried them
//                     { t: 'play' | 'settings' | 'host', id, ...body }   answered by { t: 'res', id, res }
//                     { t: 'resync' }                     a gap or a stale copy: send 'welcome' again
//                     { t: 'hb', at }                     liveness and round-trip time; answered by 'pong'
//                     { t: 'diag', ... }                  client diagnostics (kept per seat, shown in metrics)
//                     { t: 'bye' }                        leave
//   host -> member    { t: 'welcome', ...poll, cfg, room }    { t: 'res', id, res }   { t: 'you', you }
//                     { t: 'pong', at }                   { t: 'err', error }
//   host -> everyone  frame (see `frame()`): the tick's record, shared parts when they changed, the inputs applied
//
// Identity is the SDK's member id: there is no sign-in, no guest ticket and no room code. One engine is one room.

import { Arena } from './arena.js';
import { DEFAULT_CONFIG } from '../shared/rules.js';
import { encodeRoster, encodeRound, encodeStats } from '../shared/codec.js';

/** @typedef {import('../shared/types.js').Config} Config */
/** @typedef {import('./arena.js').Seat} Seat */

/** At most this many people seated in one room. */
export const MAX_PEOPLE = 24;
/** Platform user ids as the SDK hands them out. */
const ID_RE = /^[A-Za-z0-9_.:-]{1,128}$/;
/** Messages a member may send per second before the host ignores the rest (a modified client must not flood the host). */
const MAX_MSGS_PER_SEC = 60;
/** A member's display name: printable, trimmed, at most 24 characters. @param {unknown} v */
export function cleanName(v) {
  return String(v ?? '')
    .replace(/[\p{C}<>]/gu, '')
    .trim()
    .slice(0, 24);
}

/**
 * @typedef {object} EngineOptions
 * @property {string} instance            the activity instance id (the room's name)
 * @property {string} hostId              the member who launched it: the only one who starts settings changes at first
 * @property {(to: string, msg: Record<string, unknown>) => void} send   addressed message to one member
 * @property {(frame: Record<string, unknown>) => void} publish           broadcast to every member (including the host's own client)
 * @property {() => number} [now]         ms clock (default performance.now)
 * @property {string} [seed]
 * @property {Partial<Config>} [cfg]
 * @property {number} [redundancy]        how many of the newest tick records every frame carries (default 1: the channel is reliable and ordered; more repeats records so a dropped frame costs nothing)
 */

export class LandioHost {
  /** @param {EngineOptions} opt */
  constructor(opt) {
    this.opt = opt;
    this.now = opt.now ?? (() => performance.now());
    this.redundancy = Math.max(1, opt.redundancy ?? 1);
    this.arena = new Arena(roomLabel(opt.instance), opt.cfg ?? {}, opt.seed ?? `${opt.instance}:${Date.now()}`, this.now());
    this.arena.instance = opt.instance;
    this.arena.hostUid = opt.hostId;
    /** @type {Map<string, Seat>} by member id */
    this.byUid = new Map();
    /** What each seat was last told about itself (JSON without the fast-changing fields), to send changes only. @type {Map<string, string>} */
    this.youSent = new Map();
    /** The newest version of each shared part that went out in a frame. */
    this.sentVer = { rv: 0, rdv: 0, gv: 0 };
    /** Messages seen from each member in the current second. @type {Map<string, {n: number, since: number}>} */
    this.rate = new Map();
    this.tickMs = 1000 / DEFAULT_CONFIG.tickHz;
    /** @type {ReturnType<typeof setTimeout> | null} */
    this.timer = null;
    this.closed = false;
    /** Bytes of broadcast frames and of addressed messages sent so far (JSON length), for the budget. */
    this.bytes = { frames: 0, addressed: 0 };
  }

  /** The room's state, for tests and tools. */
  get state() {
    return this.arena.state;
  }

  /** Run the tick loop on the wall clock (drift-corrected, as the server's was). */
  start() {
    if (this.timer || this.closed) return;
    let next = this.now() + this.tickMs;
    const loop = () => {
      if (this.closed) return;
      const t = this.now();
      let n = 0;
      while (t >= next && n < 5) {
        this.tick();
        next += this.tickMs;
        n++;
      }
      if (t - next > this.tickMs * 5) next = t + this.tickMs; // fell far behind (a hidden tab): do not race
      this.timer = setTimeout(loop, Math.max(0, next - this.now()));
    };
    this.timer = setTimeout(loop, this.tickMs);
  }

  stop() {
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
  }

  close() {
    this.closed = true;
    this.stop();
  }

  /** Advance one tick and tell everyone. Public so tests can drive the clock. */
  tick() {
    const a = this.arena;
    // Nobody seated: nothing to play for (the room is paused, as an empty server room was).
    if (!a.seats.size) return;
    a.tick(this.now());
    this.prune();
    this.sendFrame();
    this.sendYous();
  }

  /** Forget seats the arena timed out (no message for 10 s) or removed. */
  prune() {
    for (const [uid, seat] of [...this.byUid])
      if (this.arena.seats.get(seat.token) !== seat) {
        this.byUid.delete(uid);
        this.youSent.delete(uid);
      }
  }

  /**
   * Who is in the activity now (the SDK's participant list). Seats whose member left go at once; names follow.
   * @param {{id: string, name: string}[]} members
   */
  setMembers(members) {
    const ids = new Set(members.map((m) => m.id));
    for (const [uid, seat] of [...this.byUid]) {
      if (ids.has(uid)) continue;
      if (uid === this.opt.hostId) continue; // the host's own client is always here while the engine runs
      this.arena.leave(seat, 'leave');
      this.byUid.delete(uid);
      this.youSent.delete(uid);
    }
  }

  /**
   * One message from a member.
   * @param {string} from the SDK member id
   * @param {string} name the member's display name from the SDK
   * @param {any} msg
   */
  handle(from, name, msg) {
    if (this.closed || !msg || typeof msg !== 'object' || typeof msg.t !== 'string') return;
    if (!ID_RE.test(from) || this.limited(from)) return;
    const a = this.arena;
    const now = this.now();
    if (msg.t === 'hello') return this.hello(from, cleanName(name) || cleanName(msg.name) || 'Player');
    this.prune();
    const seat = this.byUid.get(from);
    if (!seat) {
      // A page that lost its seat (the host timed it out): tell it, so it says hello again.
      this.reply(from, { t: 'err', error: 'no-seat' });
      return;
    }
    seat.lastSeen = now;
    switch (msg.t) {
      case 'in':
        a.input(seat, msg);
        break;
      case 'play':
        this.reply(from, { t: 'res', id: msg.id, res: a.play(seat) });
        this.sendYous();
        break;
      case 'settings':
        this.reply(from, { t: 'res', id: msg.id, res: a.settings(seat, msg.body ?? {}) });
        this.sendYous();
        break;
      case 'host': {
        const body = msg.body ?? {};
        // The seat to remove or promote is named by its ref (from GameInfo.seats); the host's own message only.
        const res = a.hostAction(seat, body);
        if (res.ok && body.action === 'kick') this.afterKick(body.to);
        this.reply(from, { t: 'res', id: msg.id, res });
        this.sendYous();
        break;
      }
      case 'resync':
        this.reply(from, this.welcome(seat));
        break;
      case 'hb':
        this.reply(from, { t: 'pong', at: msg.at });
        break;
      case 'diag': {
        const { t: _t, hiccups, ...rest } = msg;
        seat.diag = { ...rest, at: Math.round(now) };
        const list = (seat.hiccups ??= []);
        if (Array.isArray(hiccups)) list.push(...hiccups.slice(-20).filter((h) => h && typeof h === 'object'));
        if (list.length > 100) list.splice(0, list.length - 100);
        break;
      }
      case 'bye':
        a.leave(seat, 'leave');
        this.byUid.delete(from);
        this.youSent.delete(from);
        break;
      default:
        break;
    }
  }

  /** @param {string} uid @param {string} name */
  hello(uid, name) {
    const a = this.arena;
    if (a.banned.has(uid)) return this.reply(uid, { t: 'err', error: 'banned' });
    if (a.seats.size >= MAX_PEOPLE && !this.byUid.has(uid)) return this.reply(uid, { t: 'err', error: 'room-full' });
    const seat = a.join(uid, name, this.now(), 'channel');
    this.byUid.set(uid, seat);
    this.youSent.delete(uid);
    this.reply(uid, this.welcome(seat));
    this.sendYous();
  }

  /** The reply to a hello or a resync: what a first poll of the HTTP server answered, plus the room's config. @param {Seat} seat */
  welcome(seat) {
    const a = this.arena;
    const poll = a.poll(seat, 0, 0, this.now());
    // The first frame after this one must not look like a gap: the welcome carries the versions it contains.
    return {
      t: 'welcome',
      ...poll,
      cfg: a.state.cfg,
      room: { code: a.id, instance: true },
      host: this.opt.hostId,
      max: MAX_PEOPLE,
    };
  }

  /** The host removed `ref`'s person: they are told once, and their page stops. @param {unknown} ref */
  afterKick(ref) {
    for (const [uid, seat] of [...this.byUid])
      if (!this.arena.seats.has(seat.token) && this.arena.banned.has(uid)) {
        this.reply(uid, { t: 'err', error: 'kicked' });
        this.byUid.delete(uid);
        this.youSent.delete(uid);
      }
    void ref;
  }

  /** @param {string} from */
  limited(from) {
    const t = this.now();
    let r = this.rate.get(from);
    if (!r || t - r.since > 1000) this.rate.set(from, (r = { n: 0, since: t }));
    return ++r.n > MAX_MSGS_PER_SEC;
  }

  /** @param {string} to @param {Record<string, unknown>} msg */
  reply(to, msg) {
    this.bytes.addressed += JSON.stringify(msg).length;
    this.opt.send(to, msg);
  }

  /**
   * The broadcast for the tick just made. Records are the arena's own tick records (tiles changed, heads,
   * objects, events); the roster, the round and the settings travel only when their version moved; `y` lists, per
   * seat, the last input applied (what own-head prediction needs).
   */
  sendFrame() {
    const a = this.arena;
    const s = a.state;
    /** @type {Record<string, unknown>} */
    const f = {
      now: s.tick,
      hz: s.cfg.tickHz,
      recs: a.history.slice(-this.redundancy),
      rv: a.rosterVer,
    };
    if (a.rosterVer !== this.sentVer.rv) {
      f.roster = encodeRoster(s);
      this.sentVer.rv = a.rosterVer;
    }
    // Stats every other tick (the leaderboard redraws a few times a second).
    if (s.tick % 2 === 0) f.stats = a.sharedPart('stats', () => encodeStats(s)).val;
    const round = a.sharedPart('round', () => encodeRound(s));
    f.rdv = round.ver;
    if (round.ver !== this.sentVer.rdv) {
      f.round = round.val;
      this.sentVer.rdv = round.ver;
    }
    const game = a.sharedPart('game', () => a.gameInfo());
    f.gv = game.ver;
    if (game.ver !== this.sentVer.gv) {
      f.game = game.val;
      this.sentVer.gv = game.ver;
    }
    /** @type {[string, number, number][]} */
    const y = [];
    for (const [uid, seat] of this.byUid) y.push([uid, seat.ackSeq, seat.ackTick]);
    f.y = y;
    this.bytes.frames += JSON.stringify(f).length;
    this.opt.publish(f);
  }

  /**
   * Tell each member about themselves when something changed (their player, hand, death, role).
   * The fast-moving fields (tick, ack, land share) are not part of the change.
   */
  sendYous() {
    const a = this.arena;
    for (const [uid, seat] of this.byUid) {
      const you = a.you(seat);
      const { ack: _ack, tick: _tick, pct: _pct, ...rest } = you;
      const key = JSON.stringify(rest);
      if (this.youSent.get(uid) === key) continue;
      this.youSent.set(uid, key);
      this.reply(uid, { t: 'you', you });
    }
  }

  /** Cost and traffic of the room, as the server's /metrics showed them. */
  metrics() {
    return { ...this.arena.metrics(), bytes: { ...this.bytes } };
  }
}

/** A short label for the instance, shown where the standalone game showed the room code. @param {string} instance */
export function roomLabel(instance) {
  const id = String(instance).replace(/[^A-Za-z0-9]/g, '').toUpperCase();
  return (id.slice(-6) || 'ROOM').padEnd(4, 'X');
}

