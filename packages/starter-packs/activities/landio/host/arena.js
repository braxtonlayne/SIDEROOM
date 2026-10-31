// @ts-check
// One arena per Activity instance: the authoritative simulation, the seats of the people in it, and a short
// history of tick records so each poll gets only what it has not seen.

import { encodeFull, encodeHand, encodeHeads, encodeObjects, encodeRound, encodeRoster, encodeStats, encodeTiles, encodeTerrain } from '../shared/codec.js';
import { takeDirty, takeTerrainDirty } from '../shared/grid.js';
import { createState, endRound, landPct, maySpawn, removePlayer, spawnPlayer, startRound, step, takeEvents } from '../shared/rules.js';
import { MODE_BY_ID, roundLength } from '../shared/modes/index.js';
import { MAP_BY_ID } from '../shared/maps/index.js';
import { offerCards } from '../shared/cards.js';

/** Sandbox arenas hand every human a fresh draft this often (ticks) while they have none waiting. */
const SANDBOX_DRAFT_TICKS = 105;
/** How long a replaced token is still recognised as replaced. */
const REPLACED_KEEP_MS = 10 * 60_000;
/** At most one restart (a new mode, map or size, or the host's Restart) per arena this often: each sends everyone the whole map. */
const RESTART_COOLDOWN_MS = 5000;

/** @typedef {import('../shared/types.js').GameState} GameState */
/** @typedef {import('../shared/types.js').Config} Config */
/** @typedef {import('../shared/types.js').Input} Input */

/** Ticks of history kept: a poll older than this gets a full snapshot. */
export const HISTORY = 100;
/** Furthest ahead (in ticks) an action may be scheduled. */
export const MAX_LEAD = 12;
/** A seat that has not polled for this long is removed. */
export const SEAT_TIMEOUT_MS = 10_000;

/**
 * @typedef {object} Seat
 * @property {string} token
 * @property {boolean} [queued]   asked to play while it could not (between rounds): plays at the next round
 * @property {number} [bytesAtHealth] bytes sent by the last health line
 * @property {boolean} [uaLogged]   the browser line was logged once
 * @property {number} [statsAt]    the tick of the stats this seat was last sent
 * @property {number} [joinedAt]   when the person first sat down (reloads keep it): who is next in line to host
 * @property {string} [ref]        a short handle for this seat in the host's tools (D-61)
 * @property {string} uid
 * @property {string} name
 * @property {number} pid          current player id, 0 while spectating
 * @property {number} lastSeen     ms
 * @property {number} inSeq        last input sequence number received
 * @property {number} ackSeq       last input sequence number applied
 * @property {number} ackTick      the tick it was applied on
 * @property {number} bytesOut
 * @property {Record<string, unknown> | null} [diag]   the client's latest diagnostics report
 * @property {Record<string, unknown>[]} [hiccups]      the client's last hiccups, newest last
 * @property {number} polls
 */

/** @typedef {{t: number, tiles: number[], heads: number[], objs: number[], ter?: number[], ev: import('../shared/types.js').GameEvent[]}} TickRecord */

export class Arena {
  /** @param {string} id @param {Partial<Config>} cfg @param {string} seed @param {number} now */
  constructor(id, cfg, seed, now) {
    this.id = id;
    /** @type {GameState} */
    // Real arenas protect a fresh spawn for 3 s (D-45); the rules default (tests) is off.
    this.state = createState({ spawnShieldTicks: 21, ...cfg }, seed, { fill: true });
    takeDirty(this.state);
    takeEvents(this.state);
    /** @type {Map<string, Seat>} by token */
    this.seats = new Map();
    /** Tokens taken over by a newer page of the same person, with when (ms); kept 10 minutes. @type {Map<string, number>} */
    this.replaced = new Map();
    /** @type {TickRecord[]} */
    this.history = [];
    /** Changes made between ticks (joins, leaves), sent with the next record. */
    this.betweenEvents = /** @type {import('../shared/types.js').GameEvent[]} */ ([]);
    /** Actions waiting for their tick, by player id. @type {Map<number, {seat: Seat, acts: {seq: number, d: number, at: number}[]}>} */
    this.pending = new Map();
    this.rosterVer = 1;
    this.rosterSig = '';
    /** The Activity instance this room belongs to ('' for a room opened by a code request), and whether it was
     * created or shared by code (then it waits longer when empty). Set by the API (D-59). */
    this.instance = '';
    this.codeRoom = false;
    /** People the host removed (D-61): their uids may not come back while the room lives; their old pages hear 'kicked'. */
    this.banned = new Set();
    /** @type {Map<string, number>} token -> when it was removed */
    this.kicked = new Map();
    /** The playtest log for this arena (server/log.js; a no-op unless the server logs). @type {(type: string, data?: Record<string, unknown>) => void} */
    this.log = () => {};
    /** The tick of the last health line, and each seat's bytes sent by then. */
    this.healthTick = 0;
    /** The host (by platform user id: a reload keeps the role) and whether only the host may change settings. */
    this.hostUid = '';
    this.hostOnly = true;
    /** When the last restart happened (ms), for RESTART_COOLDOWN_MS. */
    this.restartedAt = -Infinity;
    /**
     * Parts of a poll made once per tick for everyone: the round and the settings (each with a version that
     * changes when its content does, so a poll carries them only when the asker's copy is older) and the stats.
     * @type {Record<string, {tick: number, val: unknown, json: string, ver: number}>}
     */
    this.shared = {};
    /** The round the clients were last told about, and the tick from which polls need a full snapshot. */
    this.roundN = this.state.round.n;
    this.fullFrom = 0;
    this.lastTickAt = now;
    this.emptySince = now;
    /** Server CPU per tick, ms. */
    this.tickCost = /** @type {number[]} */ ([]);
    /** Long polls waiting for the next tick. @type {Set<{resolve: () => void, timer: ReturnType<typeof setTimeout>}>} */
    this.waiters = new Set();
    this.updateRoster();
  }

  updateRoster() {
    const sig = this.state.players.map((p) => `${p.id}:${p.name}:${p.look}`).join(',');
    if (sig !== this.rosterSig) {
      this.rosterSig = sig;
      this.rosterVer++;
    }
  }

  /**
   * Seat a person (or give a returning person their seat back with a new token). `via`: how they came in ('channel',
   * 'code', 'create'), for the log. @param {string} uid @param {string} name @param {number} now @param {string} [via]
   */
  join(uid, name, now, via = 'channel') {
    for (const [tok, seat] of this.seats)
      if (seat.uid === uid) {
        // A reload (or a second window): same person, new token; their player (if any) carries on. The old
        // token is remembered so its page hears 'replaced' and stops, rather than joining back.
        this.seats.delete(tok);
        for (const [old, at] of this.replaced) if (now - at > REPLACED_KEEP_MS) this.replaced.delete(old);
        this.replaced.set(tok, now);
        seat.token = newToken();
        seat.name = name;
        seat.lastSeen = now;
        // The reloaded page counts its inputs from 1 again.
        seat.inSeq = 0;
        seat.ackSeq = 0;
        this.seats.set(seat.token, seat);
        this.log('rejoin', { uid, name });
        return seat;
      }
    /** @type {Seat} */
    const seat = { token: newToken(), uid, name, pid: 0, lastSeen: now, inSeq: 0, ackSeq: 0, ackTick: 0, bytesOut: 0, polls: 0, joinedAt: now, ref: randomHex(3) };
    this.seats.set(seat.token, seat);
    this.state.humans = this.seats.size;
    // The first person in an arena hosts it.
    if (!this.hostUid) this.hostUid = uid;
    this.log('join', { uid, name, via, host: this.hostUid === uid, people: this.seats.size });
    this.betweenEvents.push({ k: 'joined', name, via });
    return seat;
  }

  /** Spawn (or respawn) the seat's player. @param {Seat} seat */
  play(seat) {
    const s = this.state;
    const cur = seat.pid ? s.players.find((p) => p.id === seat.pid) : undefined;
    if (cur && cur.alive) return { ok: true, pid: cur.id, tick: cur.spawnTick };
    // Between rounds, or after dying in a mode without respawns: in at the next round's start.
    if (!maySpawn(s, cur)) {
      seat.queued = true;
      return { ok: false, error: 'wait', next: s.round.phase === 'over' ? s.round.next : s.round.end };
    }
    if (cur) removePlayer(s, cur.id);
    const p = spawnPlayer(s, { kind: 'human', name: seat.name, uid: seat.uid });
    if (!p) return { ok: false, error: 'full' };
    seat.pid = p.id;
    this.log('spawn', { name: seat.name, mode: s.cfg.mode, map: s.cfg.map });
    this.pending.delete(p.id);
    if (s.cfg.sandbox) offerCards(s, p, true);
    this.betweenEvents.push(...takeEvents(s));
    this.updateRoster();
    // Ids are reused: the client ignores events about this id from ticks up to `tick` (an earlier owner's).
    return { ok: true, pid: p.id, tick: s.tick };
  }

  /** @param {Seat} seat @param {'leave' | 'timeout' | 'kick'} [why] */
  leave(seat, why = 'leave') {
    this.betweenEvents.push({ k: 'left', name: seat.name, why });
    this.log('leave', { uid: seat.uid, name: seat.name, why, minutes: +(((performance.now() - (seat.joinedAt ?? 0)) / 60000).toFixed(1)) });
    if (seat.pid) removePlayer(this.state, seat.pid);
    this.seats.delete(seat.token);
    // The host left: whoever has been here longest hosts now.
    if (seat.uid === this.hostUid) {
      let next = null;
      for (const s2 of this.seats.values()) if (!next || (s2.joinedAt ?? 0) < (next.joinedAt ?? 0)) next = s2;
      this.hostUid = next ? next.uid : '';
      if (next) this.state.events.push({ k: 'host', name: next.name });
      if (next) this.log('host', { to: next.name, why: 'host left' });
      this.refreshShared();
    }
    this.pending.forEach((v, k) => v.seat === seat && this.pending.delete(k));
    this.state.humans = this.seats.size;
    this.betweenEvents.push(...takeEvents(this.state));
    this.updateRoster();
  }

  /**
   * Buffer input. `acts` are the client's last few actions as [seq, dir, at?] (dir -1 = stop toggle). Each
   * sequence number is taken once, whatever order the POSTs arrive in. `at` is the tick the client wants
   * the action applied on (a predicting client aims at the tile it showed); an action whose tick has passed
   * applies on the next tick, and one too far ahead is pulled in to at most MAX_LEAD ticks.
   * @param {Seat} seat @param {{acts?: unknown}} body
   */
  input(seat, body) {
    if (!seat.pid || !Array.isArray(body.acts)) return;
    let e = this.pending.get(seat.pid);
    if (!e) {
      e = { seat, acts: [] };
      this.pending.set(seat.pid, e);
    }
    const next = this.state.tick + 1;
    const acts = body.acts
      .slice(-8)
      .filter((a) => Array.isArray(a) && Number.isInteger(a[0]) && a[0] > seat.inSeq && Number.isInteger(a[1]) && a[1] >= -8 && a[1] <= 3)
      .sort((a, b) => a[0] - b[0]);
    for (const [seq, d, at] of acts) {
      const tick = Number.isInteger(at) ? Math.min(Math.max(at, next), next + MAX_LEAD) : next;
      // An action aimed at a tick that already has one replaces it (the client's buffer is full and the
      // newest press wins, as in the simulation's turn queue).
      // (Turns and stop/go only: a card action never replaces a turn, nor a turn a card action.)
      const same = Number.isInteger(at) && d >= -1 ? e.acts.find((x) => x.at === tick && x.d >= -1) : undefined;
      if (same) {
        same.seq = seq;
        same.d = d;
      } else e.acts.push({ seq, d, at: tick });
      seat.inSeq = seq;
    }
  }

  /** Resolve after the next tick, or after `ms` at the latest. @param {number} ms @returns {Promise<void>} */
  nextTick(ms) {
    return new Promise((resolve) => {
      const w = {
        resolve,
        timer: setTimeout(() => {
          this.waiters.delete(w);
          resolve();
        }, ms),
      };
      this.waiters.add(w);
    });
  }

  /** Advance one tick. @param {number} now */
  tick(now) {
    const s = this.state;
    const t0 = performance.now();
    /** @type {Input[]} */
    const inputs = [];
    const T = s.tick + 1;
    for (const [pid, e] of this.pending) {
      const due = e.acts.filter((a) => a.at <= T);
      if (!due.length) continue;
      e.acts = e.acts.filter((a) => a.at > T);
      /** @type {number[]} */
      const turns = [];
      /** @type {number[]} */
      const cards = [];
      let toggle = false;
      for (const a of due) {
        if (a.d === -1) toggle = !toggle;
        else if (a.d >= 0) turns.push(a.d);
        else cards.push(a.d);
      }
      inputs.push({ id: pid, turns, toggleStop: toggle, cards });
      e.seat.ackSeq = Math.max(e.seat.ackSeq, ...due.map((a) => a.seq));
      e.seat.ackTick = T;
      if (!e.acts.length) this.pending.delete(pid);
    }
    step(s, inputs);
    if (s.round.n !== this.roundN) this.newRound();
    if (s.cfg.sandbox && s.tick % SANDBOX_DRAFT_TICKS === 0)
      for (const p of s.players) if (p.alive && p.kind === 'human' && !p.offers.length) offerCards(s, p, true);
    const changed = takeDirty(s);
    const terrain = takeTerrainDirty(s);
    /** @type {TickRecord} */
    const rec = {
      t: s.tick,
      tiles: encodeTiles(s, changed),
      heads: encodeHeads(s),
      objs: encodeObjects(s),
      ...(terrain.length ? { ter: encodeTerrain(s, terrain) } : {}),
      ev: [...this.betweenEvents, ...takeEvents(s)].filter((e) => e.k !== 'capture' || (e.n ?? 0) > 0),
    };
    this.betweenEvents = [];
    this.logTick(rec);
    this.history.push(rec);
    if (this.history.length > HISTORY) this.history.shift();
    this.updateRoster();
    this.lastTickAt = now;
    // Seats that stopped polling leave.
    for (const seat of [...this.seats.values()]) if (now - seat.lastSeen > SEAT_TIMEOUT_MS) this.leave(seat, 'timeout');
    if (this.seats.size) this.emptySince = now;
    this.tickCost.push(performance.now() - t0);
    if (this.tickCost.length > 600) this.tickCost.shift();
    for (const w of this.waiters) {
      clearTimeout(w.timer);
      w.resolve();
    }
    this.waiters.clear();
  }

  /**
   * What a poll returns: the records after `since` (or a full snapshot when `since` is too old), the roster
   * when it changed since `rv`, everyone's stats and the asker's own state.
   * @param {Seat} seat @param {number} since @param {number} rv @param {number} now
   * @param {{gv: number, rdv: number}} [have] the versions of the settings and the round the asker has
   */
  poll(seat, since, rv, now, have = { gv: 0, rdv: 0 }) {
    const s = this.state;
    seat.lastSeen = now;
    seat.polls++;
    /** @type {Record<string, unknown>} */
    const out = { ok: true, now: s.tick, ms: Math.round(now - this.lastTickAt), hz: s.cfg.tickHz };
    const oldest = this.history.length ? this.history[0].t : s.tick + 1;
    // since 0 means "I have nothing" (a join, a resync): always a full snapshot, even before tick 1.
    // A new round rebuilt the map: anyone who has not seen its start gets it in full.
    if (since <= 0 || since < oldest - 1 || since < this.fullFrom) out.full = encodeFull(s);
    else if (since >= s.tick) out.recs = [];
    else out.recs = this.history.filter((r) => r.t > since);
    if (rv !== this.rosterVer || out.full) {
      out.rv = this.rosterVer;
      out.roster = encodeRoster(s);
    }
    // Stats every other tick (the leaderboard redraws a few times a second), and always with a snapshot.
    if (out.full || seat.statsAt === undefined || s.tick - seat.statsAt >= 2 || s.tick < seat.statsAt) {
      out.stats = this.sharedPart('stats', () => encodeStats(s)).val;
      seat.statsAt = s.tick;
    }
    const round = this.sharedPart('round', () => encodeRound(s));
    if (have.rdv !== round.ver || out.full) {
      out.round = round.val;
      out.rdv = round.ver;
    }
    const game = this.sharedPart('game', () => this.gameInfo());
    if (have.gv !== game.ver || out.full) {
      out.game = game.val;
      out.gv = game.ver;
    }
    out.you = this.you(seat);
    return out;
  }

  /**
   * What the asker is told about themselves: their player, the last input applied, their hand. In the
   * activity (host-authoritative) the host sends the parts that change to that person alone.
   * @param {Seat} seat
   */
  you(seat) {
    const s = this.state;
    const me = seat.pid ? s.players.find((p) => p.id === seat.pid) : undefined;
    return {
      pid: me ? me.id : 0,
      alive: !!me?.alive,
      death: me && !me.alive ? me.death : null,
      ack: [seat.ackSeq, seat.ackTick],
      pct: me ? Math.round(landPct(s, me) * 100) / 100 : 0,
      spawn: me ? me.spawnTick : 0,
      queued: !!seat.queued,
      host: seat.uid === this.hostUid,
      hand: me && me.alive ? encodeHand(me) : null,
      tick: s.tick,
    };
  }

  /**
   * What a tick record says that a playtest wants kept: deaths with a person in them, a round's results, and
   * a health line once a minute. Cheap: a scan of the tick's few events. @param {TickRecord} rec
   */
  logTick(rec) {
    const s = this.state;
    if (rec.ev.length) {
      /** @type {Set<number>} */
      const people = new Set();
      for (const seat of this.seats.values()) if (seat.pid) people.add(seat.pid);
      for (const e of rec.ev) {
        if (e.k === 'kill' && e.c !== 'removed' && (people.has(e.v ?? 0) || people.has(e.id ?? 0)))
          this.log('death', { victim: e.vn, killer: e.name || null, cause: e.c, inherited: e.n ?? 0, tick: rec.t });
        else if (e.k === 'round' && e.phase === 'over' && s.round.result) {
          const r = s.round.result;
          const top = r.players.slice(0, 5).map((p) => [p.name, p.score, p.bot ? 'bot' : 'person']);
          const win = r.winner < 0 ? `team ${-r.winner}` : r.players.find((p) => p.id === r.winner)?.name ?? null;
          this.log('round', { n: s.round.n, mode: s.cfg.mode, map: s.cfg.map, minutes: +((s.tick - s.round.start) / s.cfg.tickHz / 60).toFixed(1), winner: win, top });
        }
      }
    }
    if (s.tick - this.healthTick >= 60 * s.cfg.tickHz) {
      this.healthTick = s.tick;
      const c = this.tickCost.slice(-60 * s.cfg.tickHz).sort((a, b) => a - b);
      const ms = (/** @type {number} */ v) => Math.round(v * 1000) / 1000;
      this.log('health', {
        tick: s.tick,
        heads: s.players.filter((p) => p.alive).length,
        people: this.seats.size,
        mode: s.cfg.mode,
        map: s.cfg.map,
        size: s.cfg.w,
        tickMsAvg: ms(c.reduce((a, b) => a + b, 0) / (c.length || 1)),
        tickMsP99: ms(c[Math.floor(c.length * 0.99)] ?? 0),
        tickMsMax: ms(c[c.length - 1] ?? 0),
        kbps: [...this.seats.values()].map((x) => {
          const sent = x.bytesOut - (x.bytesAtHealth ?? 0);
          x.bytesAtHealth = x.bytesOut;
          return [x.name, Math.round(sent / 60 / 102.4) / 10];
        }),
      });
    }
  }

  /**
   * A round (re)started: everyone's client needs the new map in full, and people who asked to play while the
   * last round ended get in now.
   */
  newRound() {
    const s = this.state;
    this.roundN = s.round.n;
    // Records up to the start tick may describe the old map (a settings change restarts between ticks).
    this.fullFrom = s.round.start + 1;
    this.pending.clear();
    for (const seat of this.seats.values()) {
      // A reloaded page counts its inputs from the round's start like a fresh one.
      seat.ackTick = s.tick;
      if (!seat.queued) continue;
      seat.queued = false;
      const p = seat.pid ? s.players.find((q) => q.id === seat.pid) : undefined;
      if (!p || !p.alive) this.play(seat);
    }
    this.betweenEvents.push(...takeEvents(s));
    this.updateRoster();
    this.refreshShared();
  }

  /**
   * Change the mode and the map; a new round starts at once on the new map. Anyone seated may do it (friends
   * on a call). @param {Seat} seat @param {Record<string, unknown>} body
   */
  settings(seat, body) {
    const s = this.state;
    if (this.hostOnly && seat.uid !== this.hostUid) return { ok: false, error: 'not-host' };
    const hz = s.cfg.tickHz;
    const c = s.cfg;
    const mode = typeof body.mode === 'string' && MODE_BY_ID[body.mode] ? body.mode : c.mode;
    const map = typeof body.map === 'string' && MAP_BY_ID[body.map] ? body.map : c.map;
    const size = Number.isInteger(body.size) && /** @type {number} */ (body.size) >= 40 && /** @type {number} */ (body.size) <= 200 ? /** @type {number} */ (body.size) : c.w;
    const bots = Number.isInteger(body.bots) ? Math.max(0, Math.min(40, /** @type {number} */ (body.bots))) : c.targetHeads;
    const skill = body.skill === 'easy' || body.skill === 'normal' || body.skill === 'hard' ? body.skill : c.botSkill;
    const roundMin = typeof body.roundMin === 'number' && body.roundMin >= 0 && body.roundMin <= 30 ? body.roundMin : c.roundTicks / (60 * hz);
    const cards = typeof body.cards === 'boolean' ? body.cards : c.cards;
    const shield = typeof body.shield === 'number' && body.shield >= 0 && body.shield <= 10 ? Math.round(body.shield * hz) : c.spawnShieldTicks;
    // Only the host decides who may change settings.
    if (typeof body.hostOnly === 'boolean' && seat.uid === this.hostUid) this.hostOnly = body.hostOnly;
    const restart = mode !== c.mode || map !== c.map || size !== c.w || size !== c.h;
    const wait = this.restartWait();
    if (restart && wait > 0) return { ok: false, error: 'too-soon', wait };
    s.cfg = {
      ...c,
      mode,
      map,
      w: size,
      h: size,
      targetHeads: bots,
      botSkill: skill,
      roundTicks: Math.round(roundMin * 60 * hz),
      cards,
      spawnShieldTicks: shield,
    };
    if (restart) {
      this.restartedAt = performance.now();
      // Everyone who was playing (or asked to) is in the new round.
      for (const seat2 of this.seats.values()) if (seat2.pid) seat2.queued = true;
      startRound(s);
      s.events.push({ k: 'settings', name: seat.name, mode, map });
      this.log('settings', { by: seat.name, mode, map, size, bots, skill, roundMin, cards, shield: shield / hz, restart: true });
      this.newRound();
    } else {
      // A new length applies to the round being played (ending it next tick at the earliest).
      if (s.round.phase === 'play' && s.round.end) s.round.end = Math.max(s.tick + 1, s.round.start + roundLength(s));
      s.events.push({ k: 'settings', name: seat.name });
      this.log('settings', { by: seat.name, bots, skill, roundMin, cards, shield: shield / hz, restart: false });
    }
    this.refreshShared();
    return { ok: true };
  }

  /**
   * The host's buttons: restart the round, end it now, or hand the host role to another seated person (by
   * their player id). @param {Seat} seat @param {{action?: unknown, to?: unknown}} body
   */
  hostAction(seat, body) {
    const s = this.state;
    if (seat.uid !== this.hostUid) return { ok: false, error: 'not-host' };
    if (body.action === 'restart') {
      const wait = this.restartWait();
      if (wait > 0) return { ok: false, error: 'too-soon', wait };
      this.restartedAt = performance.now();
      for (const seat2 of this.seats.values()) if (seat2.pid) seat2.queued = true;
      startRound(s);
      s.events.push({ k: 'host', name: seat.name, phase: 'restart' });
      this.newRound();
    } else if (body.action === 'end') {
      if (s.round.phase === 'play') endRound(s);
    } else if (body.action === 'kick') {
      // Remove someone (D-61): their seat goes, and they may not come back while the room lives.
      const who = [...this.seats.values()].find((x) => x.ref === body.to && x !== seat);
      if (!who) return { ok: false, error: 'no-such-player' };
      this.banned.add(who.uid);
      this.kicked.set(who.token, performance.now());
      for (const [tok, at] of this.kicked) if (performance.now() - at > 10 * 60_000) this.kicked.delete(tok);
      this.leave(who, 'kick');
      this.log('kick', { by: seat.name, who: who.name, uid: who.uid });
    } else if (body.action === 'host') {
      const to = [...this.seats.values()].find((x) => ((x.pid && x.pid === body.to) || (x.ref && x.ref === body.to)) && x !== seat);
      if (!to) return { ok: false, error: 'no-such-player' };
      this.hostUid = to.uid;
      this.log('host', { to: to.name, by: seat.name, why: 'handed over' });
      s.events.push({ k: 'host', name: to.name });
    } else return { ok: false, error: 'action' };
    if (body.action === 'restart' || body.action === 'end') this.log('host-action', { by: seat.name, action: body.action });
    this.refreshShared();
    return { ok: true };
  }

  /**
   * A part of the poll shared by everyone, made at most once per tick; its version goes up when its content
   * changes. @param {string} key @param {() => unknown} make
   */
  sharedPart(key, make) {
    const tick = this.state.tick;
    let c = this.shared[key];
    if (!c) c = this.shared[key] = { tick: -1, val: null, json: '', ver: 0 };
    if (c.tick !== tick) {
      const val = make();
      const json = JSON.stringify(val);
      if (json !== c.json) {
        c.json = json;
        c.ver++;
      }
      c.val = val;
      c.tick = tick;
    }
    return c;
  }

  /** Milliseconds until another restart is allowed (0: now). */
  restartWait() {
    return Math.max(0, Math.ceil(this.restartedAt + RESTART_COOLDOWN_MS - performance.now()));
  }

  /** Settings or the round changed between ticks: make the shared parts again on the next poll. */
  refreshShared() {
    for (const c of Object.values(this.shared)) c.tick = -1;
  }

  /** The arena's settings and its host, for the settings menu (sent when they change). */
  gameInfo() {
    const c = this.state.cfg;
    let host = '';
    let hostPid = 0;
    for (const x of this.seats.values())
      if (x.uid === this.hostUid) {
        host = x.name;
        hostPid = x.pid;
      }
    return {
      code: this.id,
      people: [...this.seats.values()].map((x) => x.name).sort((a, b) => a.localeCompare(b)),
      // Handle, name and host flag of each seated person, for the host's tools (Remove, Make host).
      seats: [...this.seats.values()].map((x) => [x.ref ?? '', x.name, x.uid === this.hostUid ? 1 : 0]).sort((a, b) => String(a[1]).localeCompare(String(b[1]))),
      host,
      hostPid,
      hostOnly: this.hostOnly,
      size: c.w,
      bots: c.targetHeads,
      skill: c.botSkill,
      roundMin: c.roundTicks / (60 * c.tickHz),
      cards: c.cards,
      shield: c.spawnShieldTicks / c.tickHz,
    };
  }

  /** Server cost summary for measurements. */
  metrics() {
    const c = [...this.tickCost].sort((a, b) => a - b);
    const avg = c.reduce((a, b) => a + b, 0) / (c.length || 1);
    return {
      id: this.id,
      tick: this.state.tick,
      w: this.state.cfg.w,
      h: this.state.cfg.h,
      heads: this.state.players.filter((p) => p.alive).length,
      seats: [...this.seats.values()].map((x) => ({
        name: x.name,
        polls: x.polls,
        bytesOut: x.bytesOut,
        diag: x.diag ?? null,
        hiccups: x.hiccups ?? [],
      })),
      tickMsAvg: Math.round(avg * 1000) / 1000,
      tickMsP99: Math.round((c[Math.floor(c.length * 0.99)] ?? 0) * 1000) / 1000,
      tickMsMax: Math.round((c[c.length - 1] ?? 0) * 1000) / 1000,
    };
  }
}

function newToken() {
  return randomHex(18);
}

/** n random bytes as hex (the browser's crypto: the host runs in a page, not in node). @param {number} n */
function randomHex(n) {
  const b = new Uint8Array(n);
  globalThis.crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, '0')).join('');
}
