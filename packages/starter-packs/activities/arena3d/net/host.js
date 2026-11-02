// @ts-check
// The host's half of the networking: it owns the authoritative World. The host is the member who launched the
// instance (`sdk.isHost`); everyone else is a peer running net/client.js.
//
// Every tick the host applies at most one queued input per peer, in slot order (an empty queue repeats the
// peer's last input), steps the world, and every `snapshotEvery` ticks publishes a snapshot that carries, per
// slot, the seq of the last input it applied: the ack a peer reconciles against. Peers are dropped after
// `timeoutTicks` of silence or on BYE, so the host needs no participant list from the platform (a guest-safe
// design: nothing here asks the platform who anyone is).

import { MAX_PLAYERS } from '../sim/world.js';
import { sanitiseInput } from '../sim/input.js';
import { InputLog } from '../sim/input-log.js';
import { MSG, decode, encodeFull, encodeRoster, encodeSnapshot, encodeWelcome } from './codec.js';
import { encodeSeats } from './seats.js';

/** @typedef {import('../sim/world.js').World} World */
/** @typedef {import('../sim/input.js').PlayerInput} PlayerInput */
/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./codec.js').RosterEntry} RosterEntry */
/** @typedef {import('./seats.js').Seat} Seat */

/**
 * D-388: what a peer that becomes the host carries over from the old one (PredictedClient.takeover()).
 * @typedef {object} Resume
 * @property {number} selfSlot                       the slot this member already plays in
 * @property {import('../sim/world.js').Snapshot} [snap]   the newest `host:snap` it heard: the world rewinds to it
 * @property {Map<number, number>} [acks]            that snapshot's per-slot acknowledged input seq
 * @property {Seat[]} [seats]                        the other players, with their member ids
 */

/**
 * @typedef {object} Peer
 * @property {string} id
 * @property {string} name
 * @property {number} slot
 * @property {{ seq: number, input: PlayerInput }[]} queue   received, not yet applied
 * @property {number} lastSeq   newest seq received
 * @property {number} ack       seq of the last input applied
 * @property {number} heardAt   world tick of the last message from this peer
 */

export class HostSession {
  /**
   * @param {{ channel: Channel, world: World, name: string, snapshotEvery?: number, maxQueue?: number,
   *   timeoutTicks?: number, resume?: Resume }} opt
   *   resume: this member takes over a running match (host handoff): the world rewinds to `resume.snap`, the old
   *   host's player goes, and every seat is kept for its member, who says hello again
   */
  constructor(opt) {
    this.channel = opt.channel;
    this.world = opt.world;
    this.snapshotEvery = opt.snapshotEvery ?? 3;
    this.maxQueue = opt.maxQueue ?? 8;
    this.timeoutTicks = opt.timeoutTicks ?? 600;
    this.selfSlot = opt.resume?.selfSlot ?? 0;
    this.selfName = opt.name;
    /** @type {Map<string, Peer>} */
    this.peers = new Map();
    this.stats = { inputsReceived: 0, inputsDropped: 0, starved: 0, snapshots: 0, bytesOut: 0 };
    /** Every applied input change, in the replay script's format (for crash reports and bug replays). */
    this.inputLog = new InputLog();
    if (opt.resume) this.resume(opt.resume);
    this.world.addPlayer(this.selfSlot);
    this.off = this.channel.onInput((from, bytes) => this.receive(from, bytes));
    this.publishRoster();
    this.publishSnapshot();
  }

  /** @param {Resume} r */
  resume(r) {
    if (r.snap) {
      this.world.applySnapshot(r.snap);
      const keep = new Set([this.selfSlot, ...(r.seats ?? []).map((s) => s.slot)]);
      // The old host's player (and anyone nobody can name) is gone; the rest wait for their hello.
      for (let slot = 0; slot < MAX_PLAYERS; slot++) if (this.world.players[slot] && !keep.has(slot)) this.world.removePlayer(slot);
    }
    for (const s of r.snap ? (r.seats ?? []) : []) {
      if (!this.world.players[s.slot] || s.slot === this.selfSlot) continue;
      const ack = r.acks?.get(s.slot) ?? 0;
      // lastSeq = ack: the peer's unacknowledged frames are applied, the ones the old host applied are not repeated.
      this.peers.set(s.id, { id: s.id, name: s.name, slot: s.slot, queue: [], lastSeq: ack, ack, heardAt: this.world.tick });
    }
  }

  /** @returns {RosterEntry[]} */
  roster() {
    const list = [{ slot: this.selfSlot, name: this.selfName, bot: false }];
    for (const p of this.peers.values()) list.push({ slot: p.slot, name: p.name, bot: false });
    return list.sort((a, b) => a.slot - b.slot);
  }

  /** @param {string} from @param {Uint8Array} bytes */
  receive(from, bytes) {
    if (from === this.channel.self.id) return; // a hello sent to the old host, delivered after this page became the host
    const m = decode(bytes);
    if (!m) return;
    const peer = this.peers.get(from);
    if (peer) peer.heardAt = this.world.tick;
    if (m.type === MSG.HELLO) {
      if (peer) {
        this.channel.sendTo(from, encodeWelcome(peer.slot, this.world.seed, this.world.tick));
        return;
      }
      const used = new Set([this.selfSlot, ...[...this.peers.values()].map((p) => p.slot)]);
      let slot = -1;
      for (let s = 0; s < MAX_PLAYERS; s++) if (!used.has(s)) { slot = s; break; }
      if (slot < 0) {
        this.channel.sendTo(from, encodeFull());
        return;
      }
      const name = (m.name || 'Player').replace(/[\u0000-\u001f]/g, '').slice(0, 24) || 'Player';
      this.peers.set(from, { id: from, name, slot, queue: [], lastSeq: 0, ack: 0, heardAt: this.world.tick });
      this.world.addPlayer(slot);
      this.channel.sendTo(from, encodeWelcome(slot, this.world.seed, this.world.tick));
      this.publishRoster();
      this.publishSnapshot();
    } else if (m.type === MSG.INPUT && peer) {
      for (const f of m.frames) {
        if (f.seq <= peer.lastSeq) continue;
        peer.queue.push({ seq: f.seq, input: sanitiseInput(f.input) });
        peer.lastSeq = f.seq;
        this.stats.inputsReceived++;
      }
      // Bound the delay a backlog adds: drop the oldest inputs beyond maxQueue (the peer reconciles).
      while (peer.queue.length > this.maxQueue) {
        const dropped = peer.queue.shift();
        if (dropped) peer.ack = dropped.seq;
        this.stats.inputsDropped++;
      }
    } else if (m.type === MSG.BYE && peer) {
      this.removePeer(from);
    }
  }

  /** @param {string} id */
  removePeer(id) {
    const peer = this.peers.get(id);
    if (!peer) return;
    this.peers.delete(id);
    this.world.removePlayer(peer.slot);
    this.inputLog.forget(peer.slot);
    this.publishRoster();
  }

  /**
   * One fixed tick. @param {PlayerInput} localInput the host player's own input
   * @param {boolean} [publish] false: skip the periodic snapshot (a long catch-up burst; the caller publishes once)
   */
  tick(localInput, publish = true) {
    this.world.setInput(this.selfSlot, localInput);
    this.inputLog.record(this.world.tick, this.selfSlot, sanitiseInput(localInput));
    const peers = [...this.peers.values()].sort((a, b) => a.slot - b.slot);
    for (const peer of peers) {
      const next = peer.queue.shift();
      if (next) {
        this.world.setInput(peer.slot, next.input);
        this.inputLog.record(this.world.tick, peer.slot, next.input);
        peer.ack = next.seq;
      } else this.stats.starved++;
    }
    this.world.step();
    for (const peer of peers) if (this.world.tick - peer.heardAt > this.timeoutTicks) this.removePeer(peer.id);
    if (publish && this.world.tick % this.snapshotEvery === 0) this.publishSnapshot();
  }

  publishSnapshot() {
    /** @type {Map<number, number>} */
    const acks = new Map();
    for (const p of this.peers.values()) acks.set(p.slot, p.ack);
    const bytes = encodeSnapshot(this.world.snapshot(), acks);
    this.stats.snapshots++;
    this.stats.bytesOut += bytes.length;
    this.channel.publish('snap', bytes);
  }

  publishRoster() {
    this.channel.publish('roster', encodeRoster(this.roster()));
    // D-388: who holds which slot, for the peer that hosts next.
    this.channel.publish('seats', encodeSeats({ host: this.selfSlot, seats: [...this.peers.values()].map((p) => ({ id: p.id, slot: p.slot, name: p.name })) }));
  }

  close() {
    this.off();
  }
}
