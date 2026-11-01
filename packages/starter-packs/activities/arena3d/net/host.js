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
import { MSG, decode, encodeFull, encodeRoster, encodeSnapshot, encodeWelcome } from './codec.js';

/** @typedef {import('../sim/world.js').World} World */
/** @typedef {import('../sim/input.js').PlayerInput} PlayerInput */
/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./codec.js').RosterEntry} RosterEntry */

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
   *   timeoutTicks?: number }} opt
   */
  constructor(opt) {
    this.channel = opt.channel;
    this.world = opt.world;
    this.snapshotEvery = opt.snapshotEvery ?? 3;
    this.maxQueue = opt.maxQueue ?? 8;
    this.timeoutTicks = opt.timeoutTicks ?? 600;
    this.selfSlot = 0;
    this.selfName = opt.name;
    /** @type {Map<string, Peer>} */
    this.peers = new Map();
    this.stats = { inputsReceived: 0, inputsDropped: 0, starved: 0, snapshots: 0, bytesOut: 0 };
    this.world.addPlayer(this.selfSlot);
    this.off = this.channel.onInput((from, bytes) => this.receive(from, bytes));
    this.publishRoster();
    this.publishSnapshot();
  }

  /** @returns {RosterEntry[]} */
  roster() {
    const list = [{ slot: this.selfSlot, name: this.selfName, bot: false }];
    for (const p of this.peers.values()) list.push({ slot: p.slot, name: p.name, bot: false });
    return list.sort((a, b) => a.slot - b.slot);
  }

  /** @param {string} from @param {Uint8Array} bytes */
  receive(from, bytes) {
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
    this.publishRoster();
  }

  /** One fixed tick. @param {PlayerInput} localInput the host player's own input */
  tick(localInput) {
    this.world.setInput(this.selfSlot, localInput);
    const peers = [...this.peers.values()].sort((a, b) => a.slot - b.slot);
    for (const peer of peers) {
      const next = peer.queue.shift();
      if (next) {
        this.world.setInput(peer.slot, next.input);
        peer.ack = next.seq;
      } else this.stats.starved++;
    }
    this.world.step();
    for (const peer of peers) if (this.world.tick - peer.heardAt > this.timeoutTicks) this.removePeer(peer.id);
    if (this.world.tick % this.snapshotEvery === 0) this.publishSnapshot();
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
  }

  close() {
    this.off();
  }
}
