// @ts-check
// The wire format: compact binary through DataView, little-endian, every message starting with the protocol
// version byte and a type byte. A decoder returns null for anything it does not understand (another version,
// a truncated or oversized message), so a peer can never crash the host with bytes. Snapshot floats are
// float32: Jolt stores float32, so that is lossless.
//
//   HELLO     peer -> host   name
//   WELCOME   host -> peer   your slot, the match seed, the tick you join at
//   FULL      host -> peer   no free slot
//   INPUT     peer -> host   the newest unacknowledged input frames, oldest first (redundant on purpose: a lost
//                            packet is covered by the next one)
//   SNAPSHOT  host -> all    the whole simulation state at one tick, plus each player's last applied input seq
//   ROSTER    host -> all    who sits in which slot
//   BYE       peer -> host   I am leaving

import { MAX_PLAYERS } from '../sim/world.js';

export const PROTOCOL_VERSION = 1;

export const MSG = Object.freeze({ HELLO: 1, WELCOME: 2, FULL: 3, INPUT: 4, SNAPSHOT: 5, ROSTER: 6, BYE: 7 });

/** The most input frames one INPUT message carries. */
export const MAX_INPUT_FRAMES = 32;
const MAX_NAME_BYTES = 24;
const MAX_BOXES = 255;

/** @typedef {import('../sim/world.js').Snapshot} Snapshot */
/** @typedef {import('../sim/input.js').PlayerInput} PlayerInput */
/** @typedef {{ seq: number, input: PlayerInput }} InputFrame */
/** @typedef {{ slot: number, name: string, bot: boolean }} RosterEntry */
/**
 * @typedef {{ type: 1, name: string }
 *   | { type: 2, slot: number, seed: number, tick: number }
 *   | { type: 3 }
 *   | { type: 4, frames: InputFrame[] }
 *   | { type: 5, snap: Snapshot, acks: Map<number, number> }
 *   | { type: 6, roster: RosterEntry[] }
 *   | { type: 7 }} Message
 */

const utf8 = new TextEncoder();
const fromUtf8 = new TextDecoder('utf-8', { fatal: false });

/** A growable little-endian writer. */
class Writer {
  constructor(size = 256) {
    this.buf = new ArrayBuffer(size);
    this.view = new DataView(this.buf);
    this.at = 0;
  }
  /** @param {number} n */
  need(n) {
    if (this.at + n <= this.buf.byteLength) return;
    const next = new ArrayBuffer(Math.max(this.buf.byteLength * 2, this.at + n));
    new Uint8Array(next).set(new Uint8Array(this.buf, 0, this.at));
    this.buf = next;
    this.view = new DataView(next);
  }
  /** @param {number} v */ u8(v) { this.need(1); this.view.setUint8(this.at, v); this.at += 1; return this; }
  /** @param {number} v */ i16(v) { this.need(2); this.view.setInt16(this.at, v, true); this.at += 2; return this; }
  /** @param {number} v */ u32(v) { this.need(4); this.view.setUint32(this.at, v >>> 0, true); this.at += 4; return this; }
  /** @param {number} v */ f32(v) { this.need(4); this.view.setFloat32(this.at, v, true); this.at += 4; return this; }
  /** @param {string} s */
  str(s) {
    let bytes = utf8.encode(s);
    if (bytes.length > MAX_NAME_BYTES) bytes = bytes.subarray(0, MAX_NAME_BYTES);
    this.u8(bytes.length);
    this.need(bytes.length);
    new Uint8Array(this.buf, this.at, bytes.length).set(bytes);
    this.at += bytes.length;
    return this;
  }
  bytes() {
    return new Uint8Array(this.buf.slice(0, this.at));
  }
}

class Reader {
  /** @param {Uint8Array} bytes */
  constructor(bytes) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.bytes = bytes;
    this.at = 0;
  }
  // DataView throws RangeError past the end; decode() turns that into null.
  u8() { const v = this.view.getUint8(this.at); this.at += 1; return v; }
  i16() { const v = this.view.getInt16(this.at, true); this.at += 2; return v; }
  u32() { const v = this.view.getUint32(this.at, true); this.at += 4; return v; }
  f32() { const v = this.view.getFloat32(this.at, true); this.at += 4; return v; }
  str() {
    const n = this.u8();
    if (n > MAX_NAME_BYTES || this.at + n > this.bytes.length) throw new RangeError('name');
    const s = fromUtf8.decode(this.bytes.subarray(this.at, this.at + n));
    this.at += n;
    return s;
  }
  /** @returns {[number, number, number]} */ v3() { return [this.f32(), this.f32(), this.f32()]; }
}

/** @param {number} type */
const header = (type) => new Writer().u8(PROTOCOL_VERSION).u8(type);

/** @param {string} name */
export const encodeHello = (name) => header(MSG.HELLO).str(name).bytes();
/** @param {number} slot @param {number} seed @param {number} tick */
export const encodeWelcome = (slot, seed, tick) => header(MSG.WELCOME).u8(slot).u32(seed).u32(tick).bytes();
export const encodeFull = () => header(MSG.FULL).bytes();
export const encodeBye = () => header(MSG.BYE).bytes();

/**
 * Consecutive input frames (seqs must increase by one), at most MAX_INPUT_FRAMES of the newest.
 * @param {InputFrame[]} frames
 */
export function encodeInput(frames) {
  const list = frames.slice(-MAX_INPUT_FRAMES);
  const w = header(MSG.INPUT).u32(list.length ? list[0].seq : 0).u8(list.length);
  for (const f of list) w.u8(f.input.buttons & 63).i16(f.input.yaw).i16(f.input.pitch);
  return w.bytes();
}

/** @param {RosterEntry[]} roster */
export function encodeRoster(roster) {
  const w = header(MSG.ROSTER).u8(roster.length);
  for (const r of roster) w.u8(r.slot).u8(r.bot ? 1 : 0).str(r.name);
  return w.bytes();
}

/**
 * @param {Snapshot} snap
 * @param {Map<number, number>} acks slot -> the seq of the last input the host applied for that slot
 */
export function encodeSnapshot(snap, acks) {
  const w = new Writer(64 + snap.players.length * 48 + snap.boxes.length * 56);
  w.u8(PROTOCOL_VERSION).u8(MSG.SNAPSHOT).u32(snap.tick).u32(snap.rng).u8(Math.min(255, snap.score)).u8(snap.players.length);
  for (const p of snap.players) {
    w.u8(p.slot).u8(p.crouched ? 1 : 0).u8(p.prevButtons).i16(p.yaw).i16(p.pitch).f32(p.dvx).f32(p.dvz);
    w.f32(p.p[0]).f32(p.p[1]).f32(p.p[2]).f32(p.v[0]).f32(p.v[1]).f32(p.v[2]).u32(acks.get(p.slot) ?? 0);
  }
  w.u8(snap.boxes.length);
  for (const b of snap.boxes) {
    w.u8(b.active ? 1 : 0);
    w.f32(b.p[0]).f32(b.p[1]).f32(b.p[2]);
    w.f32(b.q[0]).f32(b.q[1]).f32(b.q[2]).f32(b.q[3]);
    w.f32(b.v[0]).f32(b.v[1]).f32(b.v[2]);
    w.f32(b.w[0]).f32(b.w[1]).f32(b.w[2]);
  }
  return w.bytes();
}

/**
 * @param {Uint8Array | null | undefined} bytes
 * @returns {Message | null}
 */
export function decode(bytes) {
  if (!bytes || bytes.length < 2 || bytes.length > 16384) return null;
  try {
    const r = new Reader(bytes);
    if (r.u8() !== PROTOCOL_VERSION) return null;
    const type = r.u8();
    /** @type {Message | null} */
    let m = null;
    switch (type) {
      case MSG.HELLO:
        m = { type, name: r.str() };
        break;
      case MSG.WELCOME: {
        const slot = r.u8();
        if (slot >= MAX_PLAYERS) return null;
        m = { type, slot, seed: r.u32(), tick: r.u32() };
        break;
      }
      case MSG.FULL:
      case MSG.BYE:
        m = { type };
        break;
      case MSG.INPUT: {
        const first = r.u32();
        const n = r.u8();
        if (n > MAX_INPUT_FRAMES) return null;
        /** @type {InputFrame[]} */
        const frames = [];
        for (let i = 0; i < n; i++) frames.push({ seq: (first + i) >>> 0, input: { buttons: r.u8() & 63, yaw: r.i16(), pitch: r.i16() } });
        m = { type, frames };
        break;
      }
      case MSG.SNAPSHOT: {
        const tick = r.u32();
        const rng = r.u32();
        const score = r.u8();
        const np = r.u8();
        if (np > MAX_PLAYERS) return null;
        /** @type {Map<number, number>} */
        const acks = new Map();
        /** @type {Snapshot['players']} */
        const players = [];
        for (let i = 0; i < np; i++) {
          const slot = r.u8();
          if (slot >= MAX_PLAYERS) return null;
          const crouched = (r.u8() & 1) === 1;
          const prevButtons = r.u8() & 63;
          const yaw = r.i16();
          const pitch = r.i16();
          const dvx = r.f32();
          const dvz = r.f32();
          const p = r.v3();
          const v = r.v3();
          acks.set(slot, r.u32());
          players.push({ slot, crouched, prevButtons, yaw, pitch, dvx, dvz, p, v });
        }
        const nb = r.u8();
        if (nb > MAX_BOXES) return null;
        /** @type {Snapshot['boxes']} */
        const boxes = [];
        for (let i = 0; i < nb; i++) {
          const active = (r.u8() & 1) === 1;
          const p = r.v3();
          const q = /** @type {[number, number, number, number]} */ ([r.f32(), r.f32(), r.f32(), r.f32()]);
          boxes.push({ active, p, q, v: r.v3(), w: r.v3() });
        }
        const all = [...players.map((p) => [...p.p, ...p.v, p.dvx, p.dvz]), ...boxes.map((b) => [...b.p, ...b.q, ...b.v, ...b.w])].flat();
        if (!all.every(Number.isFinite)) return null;
        m = { type, snap: { tick, rng, score, players, boxes }, acks };
        break;
      }
      case MSG.ROSTER: {
        const n = r.u8();
        if (n > MAX_PLAYERS) return null;
        /** @type {RosterEntry[]} */
        const roster = [];
        for (let i = 0; i < n; i++) {
          const slot = r.u8();
          const bot = r.u8() === 1;
          roster.push({ slot, bot, name: r.str() });
        }
        m = { type, roster };
        break;
      }
      default:
        return null;
    }
    // Trailing bytes mean the message is not what it claims to be.
    return r.at === bytes.length ? m : null;
  } catch {
    return null;
  }
}
