// @ts-check
// An in-process Channel hub: N members, one of them the host, with the semantics the SDK's data channel has
// (the host's `sdk.state` writes reach everyone and are replayed to a newcomer, `toHost` reaches the host,
// `sendTo` reaches one member). Messages go through the same JSON bytes the wire uses, are delivered
// asynchronously, and can be delayed or dropped, so the host/guest tests exercise the real encoding.

import { decodeMessage, encodeMessage } from './channel.js';

/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./channel.js').Member} Member */

/**
 * @param {{ hostId: string, latencyMs?: number, drop?: (kind: 'state' | 'input' | 'reply', msg: any) => boolean,
 *   schedule?: (fn: () => void, ms: number) => void }} opt
 */
export function createLoopbackHub(opt) {
  const schedule = opt.schedule ?? ((fn, ms) => (ms > 0 ? setTimeout(fn, ms) : queueMicrotask(fn)));
  /** @type {Map<string, {member: Member, ch: Channel, state: Set<Function>, input: Set<Function>, reply: Set<Function>, members: Set<Function>}>} */
  const peers = new Map();
  /** The shared state document: key -> value (bytes, as the wire carries it). @type {Map<string, Uint8Array>} */
  const doc = new Map();
  const stats = { stateBytes: 0, inputBytes: 0, replyBytes: 0, dropped: 0 };
  const list = () => [...peers.values()].map((p) => p.member);
  const later = (/** @type {() => void} */ fn) => schedule(fn, opt.latencyMs ?? 0);

  /** @param {string} id @param {string} name @returns {Channel} */
  function join(id, name) {
    const member = { id, name };
    const mine = { member, ch: /** @type {any} */ (null), state: new Set(), input: new Set(), reply: new Set(), members: new Set() };
    /** @type {Channel} */
    const ch = {
      self: member,
      hostId: opt.hostId,
      isHost: id === opt.hostId,
      members: list,
      onMembers: (cb) => (mine.members.add(cb), () => mine.members.delete(cb)),
      toHost(msg) {
        const bytes = encodeMessage(msg);
        stats.inputBytes += bytes.length;
        if (opt.drop?.('input', msg)) return void stats.dropped++;
        later(() => {
          const host = peers.get(opt.hostId);
          const m = decodeMessage(bytes);
          host?.input.forEach((cb) => cb(id, m));
        });
      },
      onInput: (cb) => (mine.input.add(cb), () => mine.input.delete(cb)),
      publish(key, value) {
        if (id !== opt.hostId) throw new Error('only the host writes host:* state');
        const bytes = encodeMessage(value);
        doc.set(key, bytes);
        stats.stateBytes += bytes.length * Math.max(1, peers.size - 1);
        if (opt.drop?.('state', value)) return void stats.dropped++;
        for (const [pid, p] of peers) {
          if (pid === id) continue;
          later(() => p.state.forEach((cb) => cb(key, decodeMessage(bytes))));
        }
      },
      onState(cb) {
        mine.state.add(cb);
        // A newcomer hears the document as it stands.
        for (const [key, bytes] of doc) later(() => cb(key, decodeMessage(bytes)));
        return () => mine.state.delete(cb);
      },
      sendTo(to, msg) {
        const bytes = encodeMessage(msg);
        stats.replyBytes += bytes.length;
        if (opt.drop?.('reply', msg)) return void stats.dropped++;
        later(() => peers.get(to)?.reply.forEach((cb) => cb(decodeMessage(bytes))));
      },
      onReply: (cb) => (mine.reply.add(cb), () => mine.reply.delete(cb)),
      close() {
        if (peers.delete(id)) announce();
      },
    };
    mine.ch = ch;
    peers.set(id, mine);
    announce();
    return ch;
  }

  function announce() {
    const l = list();
    for (const p of peers.values()) later(() => p.members.forEach((cb) => cb(l)));
  }

  return { join, stats, doc, peers: () => [...peers.keys()] };
}
