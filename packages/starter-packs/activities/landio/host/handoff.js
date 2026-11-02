// @ts-check
// Host migration, the game half (D-388, ACT-LND-002). Land.io runs in the browser of the member who launched it.
// When that member leaves, someone else has to carry on the same game. The platform does its part from wave 46 for a
// bundle that declares `host_handoff`: the server names the successor with the same rule as `successor()` below, the
// client sends HOST_CHANGED, the SDK moves `host:*` writes to the new host, and client/net/transport.js calls
// `HostHandoff.hostChanged` (snapshots on, HANDOFF_SNAPSHOT_TICKS). This file is the game half:
//
//   successor(candidates, leaving)  who hosts next: the longest-connected member, ties broken by member id. Every
//                                   member computes the same answer from the same published seat order.
//   snapshotEngine(engine)          a compact, JSON-safe snapshot of the running game (the simulation, seats,
//                                   room settings) that a successor can resume from.
//   resumeEngine(snapshot, opt)     the successor's engine, built from a snapshot: stepping it gives exactly the
//                                   ticks the old host would have given (tests: test/handoff.test.js).
//   HostHandoff                     the interface the platform calls (implemented by net/sdk-transport.js).
//
// No member data is in a snapshot beyond what every member already sees in the game (seat names, member ids of
// the seats): it never holds platform tokens.

import { LandioHost } from './engine.js';
import { rleDecode, rleEncode } from '../shared/codec.js';

/** @typedef {import('../shared/types.js').GameState} GameState */
/** @typedef {import('./arena.js').Seat} Seat */

/** Snapshot format version: a successor refuses a snapshot it does not understand (a stale page). */
export const SNAPSHOT_VERSION = 1;

/**
 * @typedef {object} SeatSnapshot
 * @property {string} uid     member id
 * @property {string} name
 * @property {number} pid     the seat's player id (0 while spectating)
 * @property {number} order   when it sat down, as a rank (0 = first): "longest connected" survives a new clock
 * @property {boolean} [queued]
 * @property {number} inSeq   the last input sequence number received (a page that keeps its seat keeps counting)
 * @property {number} ackSeq
 * @property {[number, number, number][]} acts   inputs received but not applied yet: [seq, dir, tick]
 */

/**
 * @typedef {object} HandoffSnapshot
 * @property {number} v                   SNAPSHOT_VERSION
 * @property {string} instance
 * @property {string} host                the host that made it
 * @property {number} tick
 * @property {Record<string, any>} state  the simulation (tile layers run-length encoded)
 * @property {SeatSnapshot[]} seats       in sitting-down order
 * @property {{hostOnly: boolean, hostUid: string, banned: string[]}} room
 */

/**
 * The interface the platform calls when it moves the host role (D-388). Land.io's transport implements it.
 * @typedef {object} HostHandoff
 * @property {() => HandoffSnapshot | null} snapshot   host: the game as it stands now; a guest: the newest one it heard
 * @property {(members: string[]) => string | null} successor   who should host next among the members still here
 * @property {(newHostId: string, leftId: string) => Promise<{ok: boolean, role: 'host' | 'guest', error?: string}>} hostChanged
 *   the platform made `newHostId` the host after `leftId` left: the new host resumes from the newest snapshot, the
 *   others say hello to it again (their seats, players and cards carry on)
 */

/**
 * Who hosts next: among `candidates` (member ids with when they sat down, smaller = earlier), not `leaving`, the
 * longest connected; on a tie the smaller member id (plain code-unit order, the same everywhere). Null when nobody
 * is left. @param {{id: string, since: number}[]} candidates @param {string} [leaving] @returns {string | null}
 */
export function successor(candidates, leaving) {
  let best = null;
  for (const c of candidates) {
    if (c.id === leaving) continue;
    if (!best || c.since < best.since || (c.since === best.since && c.id < best.id)) best = c;
  }
  return best ? best.id : null;
}

/**
 * The successor among the members still in the call, from a snapshot's seat order: seated members first by when
 * they sat down, members without a seat after them (all tied, so by id). @param {HandoffSnapshot | null} snap
 * @param {string[]} members @param {string} [leaving]
 */
export function successorFromSnapshot(snap, members, leaving) {
  const order = new Map((snap?.seats ?? []).map((s) => [s.uid, s.order]));
  return successor(
    members.map((id) => ({ id, since: order.get(id) ?? Number.MAX_SAFE_INTEGER })),
    leaving ?? snap?.host,
  );
}

/** The simulation as plain JSON. @param {GameState} s */
export function snapshotState(s) {
  return {
    cfg: s.cfg,
    tick: s.tick,
    rng: s.rng,
    base: rleEncode(s.base),
    trail: rleEncode(s.trail),
    terrain: rleEncode(s.terrain),
    players: s.players,
    humans: s.humans,
    botCooldown: s.botCooldown,
    objects: s.objects,
    nextObjectId: s.nextObjectId,
    seed: s.seed,
    round: s.round,
    md: s.md,
  };
}

/**
 * A GameState from `snapshotState` output (a deep copy: the snapshot is not shared with the new state).
 * @param {Record<string, any>} j @returns {GameState}
 */
export function restoreState(j) {
  const c = JSON.parse(JSON.stringify(j));
  const n = c.cfg.w * c.cfg.h;
  return {
    cfg: c.cfg,
    tick: c.tick,
    rng: c.rng,
    base: rleDecode(c.base, new Uint16Array(n)),
    trail: rleDecode(c.trail, new Uint16Array(n)),
    terrain: rleDecode(c.terrain, new Uint8Array(n)),
    terrainDirty: [],
    players: c.players,
    humans: c.humans,
    botCooldown: c.botCooldown,
    events: [],
    dirtyMark: new Uint8Array(n),
    dirty: [],
    objects: c.objects,
    nextObjectId: c.nextObjectId,
    seed: c.seed,
    round: c.round,
    md: c.md,
  };
}

/**
 * The running game as a snapshot (call between ticks). @param {LandioHost} engine @returns {HandoffSnapshot}
 */
export function snapshotEngine(engine) {
  const a = engine.arena;
  const seats = [...a.seats.values()].sort((x, y) => (x.joinedAt ?? 0) - (y.joinedAt ?? 0) || (x.uid < y.uid ? -1 : x.uid > y.uid ? 1 : 0));
  return {
    v: SNAPSHOT_VERSION,
    instance: a.instance,
    host: engine.opt.hostId,
    tick: a.state.tick,
    state: snapshotState(a.state),
    seats: seats.map((x, order) => ({
      uid: x.uid,
      name: x.name,
      pid: x.pid,
      order,
      ...(x.queued ? { queued: true } : {}),
      inSeq: x.inSeq,
      ackSeq: x.ackSeq,
      acts: /** @type {[number, number, number][]} */ ((x.pid && a.pending.get(x.pid)?.seat === x ? a.pending.get(x.pid)?.acts ?? [] : []).map((e) => [e.seq, e.d, e.at])),
    })),
    room: { hostOnly: a.hostOnly, hostUid: a.hostUid, banned: [...a.banned] },
  };
}

/**
 * The successor's engine: the snapshot's game, with every seat that was in it (so members who say hello again get
 * their player back) except `leaving`, whose player leaves the arena as a person leaving would. The engine is not
 * started: the caller wires it to its channel and starts it.
 * @param {HandoffSnapshot} snap
 * @param {import('./engine.js').EngineOptions & {leaving?: string}} opt   opt.hostId: the new host
 * @returns {LandioHost}
 */
export function resumeEngine(snap, opt) {
  if (!snap || snap.v !== SNAPSHOT_VERSION) throw new Error('handoff: unknown snapshot version');
  const engine = new LandioHost({ ...opt, instance: opt.instance ?? snap.instance, cfg: snap.state.cfg, seed: snap.state.seed });
  const a = engine.arena;
  a.state = restoreState(snap.state);
  a.roundN = a.state.round.n;
  a.fullFrom = a.state.tick + 1;
  a.history = [];
  a.pending.clear();
  a.hostOnly = snap.room.hostOnly;
  a.banned = new Set(snap.room.banned);
  const now = engine.now();
  // Seats come back in their order (joinedAt keeps it on the new clock), with fresh tokens.
  for (const x of snap.seats) {
    const seat = a.join(x.uid, x.name, now - (snap.seats.length - x.order), 'handoff');
    seat.pid = x.pid;
    seat.queued = !!x.queued;
    seat.ackTick = a.state.tick;
    seat.inSeq = x.inSeq;
    seat.ackSeq = x.ackSeq;
    if (x.pid && x.acts.length) a.pending.set(x.pid, { seat, acts: x.acts.map(([seq, d, at]) => ({ seq, d, at })) });
    engine.byUid.set(x.uid, seat);
  }
  // join() counted people for the bot quota and queued 'joined' events: restore the snapshot's own count, and
  // keep the between-tick events out of the simulation (they were seen before the snapshot).
  a.state.humans = snap.state.humans;
  a.betweenEvents = [];
  a.hostUid = snap.room.hostUid === snap.host ? opt.hostId : snap.room.hostUid;
  const left = opt.leaving ?? snap.host;
  const gone = engine.byUid.get(left);
  if (gone && left !== opt.hostId) {
    a.leave(gone, 'leave');
    engine.byUid.delete(left);
  }
  // A new host with no seat in the snapshot (it was still in the menu) sits down with its hello, like anyone.
  a.updateRoster();
  a.refreshShared();
  return engine;
}
