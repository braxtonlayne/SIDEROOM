// @ts-check
// A Channel between browser tabs of one origin (BroadcastChannel), for developing without a SIDE ROOM client:
// open dist/index.html?user=alice&instance=room1 in two tabs. It stands in for the SDK's data channel. The first
// tab of an instance hosts it (a short election at start-up). No security and no E2EE: it is only the local
// development stand-in, and it never runs inside SIDE ROOM (the page picks platform/sideroom-platform.js there).

/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./channel.js').Member} Member */

const BEAT_MS = 1000;
const ELECT_MS = 450;

/**
 * @param {{ instance: string, self: Member, win?: Window }} opt
 * @returns {Promise<Channel>}
 */
export async function createBroadcastChannel(opt) {
  const bc = new BroadcastChannel(`arena3d-dev-${opt.instance}`);
  const self = opt.self;
  /** @type {Map<string, { host: boolean }>} */
  const peers = new Map();
  const inputCbs = new Set();
  const stateCbs = new Set();
  const replyCbs = new Set();
  /** @type {Map<string, Uint8Array>} the newest value of each host key, replayed to a newcomer */
  const doc = new Map();
  let isHost = false;
  let hostId = '';

  /** @param {object} m */
  const post = (m) => bc.postMessage({ from: self.id, ...m });
  const presence = () => post({ k: 'presence', host: isHost });

  bc.onmessage = (e) => {
    const m = e.data;
    if (!m || typeof m.from !== 'string' || m.from === self.id) return;
    if (m.k === 'presence') {
      const known = peers.has(m.from);
      peers.set(m.from, { host: !!m.host });
      if (m.host && !(isHost && self.id < m.from)) {
        // Two tabs that elected themselves at once: the lower id keeps hosting.
        hostId = m.from;
        isHost = false;
      }
      if (!known) {
        presence();
        if (isHost) for (const [key, bytes] of doc) post({ k: 'state', key, bytes, to: m.from });
      }
    } else if (m.k === 'bye') {
      peers.delete(m.from);
      // The host left: the remaining tab with the lowest id takes over (a dev stand-in; state restarts there).
      if (m.from === hostId && [...peers.keys()].every((id) => self.id < id)) {
        isHost = true;
        hostId = self.id;
        presence();
      }
    }
    else if (m.k === 'in' && isHost && m.to === self.id) inputCbs.forEach((cb) => cb(m.from, new Uint8Array(m.bytes)));
    else if (m.k === 'reply' && m.to === self.id) replyCbs.forEach((cb) => cb(new Uint8Array(m.bytes)));
    else if (m.k === 'state' && (m.to === undefined || m.to === self.id) && m.from === hostId)
      stateCbs.forEach((cb) => cb(m.key, new Uint8Array(m.bytes)));
  };

  presence();
  await new Promise((r) => setTimeout(r, ELECT_MS));
  isHost = ![...peers.values()].some((p) => p.host);
  if (isHost) hostId = self.id;
  presence();
  const beat = setInterval(presence, BEAT_MS);
  opt.win?.addEventListener('pagehide', () => post({ k: 'bye' }));

  return {
    self,
    get hostId() {
      return hostId;
    },
    get isHost() {
      return isHost;
    },
    toHost: (bytes) => post({ k: 'in', to: hostId, bytes }),
    onInput: (cb) => (inputCbs.add(cb), () => inputCbs.delete(cb)),
    publish(key, bytes) {
      doc.set(key, bytes);
      post({ k: 'state', key, bytes });
    },
    onState(cb) {
      stateCbs.add(cb);
      return () => stateCbs.delete(cb);
    },
    sendTo: (to, bytes) => post({ k: 'reply', to, bytes }),
    onReply: (cb) => (replyCbs.add(cb), () => replyCbs.delete(cb)),
    close() {
      clearInterval(beat);
      post({ k: 'bye' });
      bc.close();
    },
  };
}
