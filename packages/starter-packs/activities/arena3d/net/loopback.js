// @ts-check
// N Channels in one process, with the data channel's semantics (host state reaches everyone and is replayed to
// a newcomer, toHost reaches the host, sendTo reaches one member) and a simulated network: latency, jitter and
// loss, all from a seeded PRNG and a virtual clock, so a test with 15% loss is still reproducible. Nothing is
// delivered until `pump(now)`: the test owns time.

import { Rng } from '../sim/rng.js';

/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {'input' | 'state' | 'reply'} Kind */

/**
 * @param {{ hostId: string, latencyMs?: number, jitterMs?: number, seed?: number,
 *   drop?: Partial<Record<Kind, number>> }} opt
 *   drop: the chance (0..1) that one message of that kind is lost
 */
export function createLoopbackHub(opt) {
  const rng = new Rng(opt.seed ?? 1);
  const latency = opt.latencyMs ?? 0;
  const jitter = opt.jitterMs ?? 0;
  let now = 0;
  let order = 0;
  /** @type {{ at: number, n: number, fn: () => void }[]} */
  let queue = [];
  /** @type {Map<string, { input: Set<Function>, state: Set<Function>, reply: Set<Function> }>} */
  const peers = new Map();
  /** @type {Map<string, Uint8Array>} */
  const doc = new Map();
  const stats = { sent: 0, dropped: 0, bytes: { input: 0, state: 0, reply: 0 } };

  /** Queue a delivery after the simulated delay, unless the network loses it. @param {Kind} kind @param {() => void} fn */
  function send(kind, fn) {
    stats.sent++;
    if (rng.range(0, 1) < (opt.drop?.[kind] ?? 0)) {
      stats.dropped++;
      return;
    }
    const at = now + latency + (jitter ? rng.range(0, jitter) : 0);
    queue.push({ at, n: order++, fn });
  }

  /** @param {string} id @param {string} name @returns {Channel} */
  function join(id, name) {
    const mine = { input: new Set(), state: new Set(), reply: new Set() };
    peers.set(id, mine);
    return {
      self: { id, name },
      hostId: opt.hostId,
      isHost: id === opt.hostId,
      toHost(bytes) {
        const copy = bytes.slice();
        stats.bytes.input += copy.length;
        send('input', () => peers.get(opt.hostId)?.input.forEach((cb) => cb(id, copy)));
      },
      onInput: (cb) => (mine.input.add(cb), () => mine.input.delete(cb)),
      publish(key, bytes) {
        if (id !== opt.hostId) throw new Error('only the host writes host:* state');
        const copy = bytes.slice();
        doc.set(key, copy);
        for (const [pid, p] of peers) {
          if (pid === id) continue;
          stats.bytes.state += copy.length;
          send('state', () => p.state.forEach((cb) => cb(key, copy)));
        }
      },
      onState(cb) {
        mine.state.add(cb);
        // A newcomer hears the document as it stands (reliably: the SDK syncs it on join).
        for (const [key, bytes] of doc) queue.push({ at: now + latency, n: order++, fn: () => cb(key, bytes) });
        return () => mine.state.delete(cb);
      },
      sendTo(to, bytes) {
        const copy = bytes.slice();
        stats.bytes.reply += copy.length;
        send('reply', () => peers.get(to)?.reply.forEach((cb) => cb(copy)));
      },
      onReply: (cb) => (mine.reply.add(cb), () => mine.reply.delete(cb)),
      close() {
        peers.delete(id);
      },
    };
  }

  /** Advance the clock to `t` (ms) and deliver everything due, in time order. @param {number} t */
  function pump(t) {
    now = Math.max(now, t);
    for (;;) {
      const due = queue.filter((q) => q.at <= now).sort((a, b) => a.at - b.at || a.n - b.n);
      if (!due.length) return;
      queue = queue.filter((q) => q.at > now);
      for (const d of due) d.fn();
    }
  }

  return { join, pump, stats, now: () => now };
}
