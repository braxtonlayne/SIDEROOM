// @ts-check
// A peer's half of the networking: client-side prediction with server reconciliation.
//
//   1. Every tick the peer samples its input, gives it the next seq, applies it to its own local World at once
//      (prediction: no waiting for the round trip) and keeps it in `pending`.
//   2. Every `sendEvery` ticks it sends the host all pending inputs (up to 32): redundancy, so a lost packet
//      is covered by the next one.
//   3. When a snapshot arrives it drops the inputs the host has applied (seq <= ack), rewinds the local World
//      to the snapshot, and re-simulates the inputs still pending. The difference between where the player was
//      drawn and where it is now becomes a visual offset that decays over a few frames, so a correction is a
//      glide, not a snap.
//   4. Remote players are drawn from net/interp.js, a little in the past, not from the predicted world.

import { MSG, decode, encodeBye, encodeHello, encodeInput } from './codec.js';
import { Interpolator } from './interp.js';
import { sanitiseInput } from '../sim/input.js';
import { InputLog } from '../sim/input-log.js';

/** @typedef {import('../sim/world.js').World} World */
/** @typedef {import('../sim/world.js').Snapshot} Snapshot */
/** @typedef {import('../sim/input.js').PlayerInput} PlayerInput */
/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./codec.js').RosterEntry} RosterEntry */

export class PredictedClient {
  /**
   * @param {{ channel: Channel, world: World, name: string, sendEvery?: number, helloEvery?: number,
   *   interpDelayTicks?: number, smoothing?: number }} opt
   */
  constructor(opt) {
    this.channel = opt.channel;
    this.world = opt.world;
    this.name = opt.name;
    this.sendEvery = opt.sendEvery ?? 2;
    this.helloEvery = opt.helloEvery ?? 30;
    /** Per-frame decay of the visual correction offset (0 = snap, 0.9 = very soft). */
    this.smoothing = opt.smoothing ?? 0.8;
    /** @type {'joining' | 'playing' | 'full'} */
    this.status = 'joining';
    /** @type {number} my slot, -1 until welcomed */
    this.slot = -1;
    this.seq = 0;
    /** @type {{ seq: number, input: PlayerInput }[]} */
    this.pending = [];
    this.lastSnapTick = -1;
    this.haveBaseline = false;
    /** The newest host tick heard, advanced by one per local tick in between: the clock for interpolation. */
    this.hostTick = 0;
    this.localTicks = 0;
    /** @type {[number, number, number]} visual correction offset for my own player */
    this.offset = [0, 0, 0];
    /** @type {RosterEntry[]} */
    this.roster = [];
    this.interp = new Interpolator({ delayTicks: opt.interpDelayTicks ?? 6 });
    /** My own predicted inputs, in the replay script's format, by local predicted tick. */
    this.inputLog = new InputLog();
    this.stats = { snapshots: 0, corrections: 0, maxCorrection: 0, lastCorrection: 0, resimTicks: 0, inputsSent: 0 };
    /** @type {(() => void)[]} */
    this.offs = [
      this.channel.onReply((bytes) => this.receive(bytes)),
      this.channel.onState((key, bytes) => {
        if (key === 'snap' || key === 'roster') this.receive(bytes);
      }),
    ];
    this.channel.toHost(encodeHello(this.name));
  }

  /** @param {Uint8Array} bytes */
  receive(bytes) {
    const m = decode(bytes);
    if (!m) return;
    if (m.type === MSG.WELCOME) {
      if (this.slot < 0) this.slot = m.slot;
      if (this.status === 'joining') this.status = 'playing';
    } else if (m.type === MSG.FULL) {
      if (this.slot < 0) this.status = 'full';
    } else if (m.type === MSG.ROSTER) {
      this.roster = m.roster;
    } else if (m.type === MSG.SNAPSHOT) {
      this.onSnapshot(m.snap, m.acks);
    }
  }

  /** @param {Snapshot} snap @param {Map<number, number>} acks */
  onSnapshot(snap, acks) {
    if (snap.tick <= this.lastSnapTick) return; // late or duplicate
    this.lastSnapTick = snap.tick;
    this.stats.snapshots++;
    this.hostTick = Math.max(this.hostTick, snap.tick);
    this.interp.push(snap);
    const mine = this.slot >= 0 ? snap.players.find((p) => p.slot === this.slot) : undefined;
    const before = mine && this.haveBaseline ? this.world.eye(this.slot) : null;
    if (mine) {
      const ack = acks.get(this.slot) ?? 0;
      this.pending = this.pending.filter((f) => f.seq > ack);
    }
    // Rewind to the host's tick, then replay what the host has not applied yet.
    this.world.applySnapshot(snap);
    this.haveBaseline = true;
    if (mine) {
      for (const f of this.pending) {
        this.world.setInput(this.slot, f.input);
        this.world.step();
        this.stats.resimTicks++;
      }
      const after = this.world.eye(this.slot);
      if (before && after) {
        const dx = before.x - after.x;
        const dy = before.y - after.y;
        const dz = before.z - after.z;
        const err = Math.hypot(dx, dy, dz);
        this.stats.lastCorrection = err;
        if (err > 1e-4) {
          this.stats.corrections++;
          this.stats.maxCorrection = Math.max(this.stats.maxCorrection, err);
        }
        // Teleports (respawns) snap; small corrections glide.
        if (err < 2) {
          this.offset[0] += dx;
          this.offset[1] += dy;
          this.offset[2] += dz;
        } else this.offset = [0, 0, 0];
      }
    }
  }

  /**
   * One fixed local tick. @param {PlayerInput} input this player's input for the tick
   */
  tick(input) {
    this.localTicks++;
    this.hostTick++;
    if (this.status === 'joining' && this.localTicks % this.helloEvery === 0) this.channel.toHost(encodeHello(this.name));
    if (this.status !== 'playing' || !this.haveBaseline || !this.world.players[this.slot]) return;
    const clean = sanitiseInput(input);
    this.seq++;
    this.pending.push({ seq: this.seq, input: clean });
    if (this.pending.length > 120) this.pending.shift(); // the host is gone or far behind; keep memory bounded
    this.world.setInput(this.slot, clean);
    this.inputLog.record(this.world.tick, this.slot, clean);
    this.world.step();
    if (this.seq % this.sendEvery === 0) {
      this.channel.toHost(encodeInput(this.pending));
      this.stats.inputsSent++;
    }
  }

  /** Call once per rendered frame: decays the correction offset. */
  frame() {
    const k = this.smoothing;
    this.offset[0] *= k;
    this.offset[1] *= k;
    this.offset[2] *= k;
  }

  /** My eye for the camera: predicted position plus the decaying correction offset. */
  myEye() {
    const e = this.world.eye(this.slot);
    if (!e) return null;
    return { ...e, x: e.x + this.offset[0], y: e.y + this.offset[1], z: e.z + this.offset[2] };
  }

  /** A remote player as drawn (interpolated). @param {number} slot @param {number} [alpha] fraction of a tick */
  remote(slot, alpha = 0) {
    return this.interp.sample(slot, this.hostTick + alpha);
  }

  leave() {
    this.channel.toHost(encodeBye());
    for (const off of this.offs.splice(0)) off();
  }
}
