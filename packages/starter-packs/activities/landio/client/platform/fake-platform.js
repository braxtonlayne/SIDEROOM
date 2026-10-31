// @ts-check
// Phase 1 stand-in for SIDE ROOM. `?user=alice` names the person (several tabs = several people) and
// `?instance=room1` the Activity instance (default "local"). Tabs of one instance find each other over a
// BroadcastChannel, which plays the part of the voice channel's participant list.

/** @typedef {import('./platform.js').Platform} Platform */
/** @typedef {import('./platform.js').Identity} Identity */

const HEARTBEAT_MS = 1000;
const GONE_MS = 3500;

/** @param {Window} win @returns {Platform} */
export function createFakePlatform(win) {
  const q = new URLSearchParams(win.location.search);
  const name = (q.get('user') || `guest${Math.floor(Math.random() * 900 + 100)}`).slice(0, 24);
  const me = { id: `fake-${name.replace(/[^A-Za-z0-9_-]/g, '_')}`, name };
  const instance = (q.get('instance') || 'local').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 64);
  /** @type {Map<string, {who: Identity, seen: number}>} */
  const peers = new Map();
  /** @type {Set<(p: Identity[]) => void>} */
  const listeners = new Set();
  /** @type {BroadcastChannel | null} */
  let chan = null;

  const list = () => [me, ...[...peers.values()].map((p) => p.who)].sort((a, b) => a.name.localeCompare(b.name));
  const emit = () => listeners.forEach((cb) => cb(list()));

  return {
    kind: 'fake',
    async ready() {
      if (typeof BroadcastChannel === 'undefined') return;
      chan = new BroadcastChannel(`landio-fake-${instance}`);
      chan.onmessage = (e) => {
        const m = e.data;
        if (!m || typeof m.id !== 'string' || m.id === me.id) return;
        if (m.bye) {
          if (peers.delete(m.id)) emit();
          return;
        }
        const known = peers.has(m.id);
        peers.set(m.id, { who: { id: m.id, name: String(m.name) }, seen: Date.now() });
        if (!known) {
          emit();
          chan?.postMessage(me); // answer a newcomer at once
        }
      };
      chan.postMessage(me);
      setInterval(() => {
        chan?.postMessage(me);
        const t = Date.now();
        let changed = false;
        for (const [id, p] of peers)
          if (t - p.seen > GONE_MS) {
            peers.delete(id);
            changed = true;
          }
        if (changed) emit();
      }, HEARTBEAT_MS);
      win.addEventListener('pagehide', () => chan?.postMessage({ id: me.id, bye: true }));
    },
    identity: () => me,
    ticket: () => null,
    participants: async () => list(),
    onParticipantsChange(cb) {
      listeners.add(cb);
      return () => listeners.delete(cb);
    },
    instanceId: () => instance,
    setActivity(text) {
      win.document.title = text ? `Land.io — ${text}` : 'Land.io';
    },
  };
}
