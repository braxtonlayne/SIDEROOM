// @ts-check
// A `Channel` between browser tabs of one origin (BroadcastChannel), for developing and demonstrating Land.io
// without a SIDE ROOM client: `?user=alice&instance=room1` in two tabs. It stands in for the SDK's data channel
// and the participant list. The first tab to open an instance hosts it (a short election at start-up); there is
// no security here and no E2EE, it is only the development stand-in.

/** @typedef {import('./channel.js').Channel} Channel */
/** @typedef {import('./channel.js').Member} Member */

const BEAT_MS = 1000;
const GONE_MS = 3500;
const ELECT_MS = 450;

/**
 * @param {{ instance: string, self: Member, win?: Window }} opt
 * @returns {Promise<Channel & { participantsApi: { list: () => Member[], on: (cb: (l: Member[]) => void) => () => void } }>}
 */
export async function createBroadcastChannel(opt) {
  const bc = new BroadcastChannel(`landio-dev-${opt.instance}`);
  const self = opt.self;
  /** @type {Map<string, {who: Member, seen: number, host: boolean}>} */
  const peers = new Map();
  /** @type {Set<(l: Member[]) => void>} */
  const memberCbs = new Set();
  const inputCbs = new Set();
  const stateCbs = new Set();
  const replyCbs = new Set();
  /** The newest value of each host key, replayed to a newcomer. @type {Map<string, any>} */
  const doc = new Map();
  let isHost = false;
  let hostId = '';

  const list = () => [self, ...[...peers.values()].map((p) => p.who)].sort((a, b) => a.name.localeCompare(b.name));
  const emit = () => memberCbs.forEach((cb) => cb(list()));
  /** @param {object} m */
  const post = (m) => bc.postMessage({ from: self.id, ...m });
  const presence = () => post({ k: 'presence', who: self, host: isHost });

  bc.onmessage = (e) => {
    const m = e.data;
    if (!m || typeof m.from !== 'string' || m.from === self.id) return;
    if (m.k === 'presence') {
      const known = peers.has(m.from);
      peers.set(m.from, { who: m.who, seen: Date.now(), host: !!m.host });
      if (m.host) hostId = m.from;
      if (!known) {
        emit();
        presence();
        // A newcomer hears the shared state as it stands.
        if (isHost) for (const [key, value] of doc) post({ k: 'state', key, value, to: m.from });
      }
    } else if (m.k === 'bye') {
      if (peers.delete(m.from)) emit();
    } else if (m.k === 'in' && isHost && m.to === self.id) inputCbs.forEach((cb) => cb(m.from, m.msg));
    else if (m.k === 'reply' && m.to === self.id) replyCbs.forEach((cb) => cb(m.msg));
    else if (m.k === 'state' && (m.to === undefined || m.to === self.id) && m.from === hostId) stateCbs.forEach((cb) => cb(m.key, m.value));
  };

  presence();
  // Election: if nobody who is already hosting answers, this tab hosts.
  await new Promise((r) => setTimeout(r, ELECT_MS));
  isHost = ![...peers.values()].some((p) => p.host);
  hostId = isHost ? self.id : (hostId || [...peers.entries()].find(([, p]) => p.host)?.[0] || '');
  presence();
  emit();
  const beat = setInterval(() => {
    presence();
    const t = Date.now();
    let changed = false;
    for (const [id, p] of peers)
      if (t - p.seen > GONE_MS) {
        peers.delete(id);
        changed = true;
      }
    if (changed) emit();
  }, BEAT_MS);
  opt.win?.addEventListener('pagehide', () => post({ k: 'bye' }));

  return {
    self,
    get hostId() {
      return hostId;
    },
    get isHost() {
      return isHost;
    },
    members: list,
    onMembers: (cb) => (memberCbs.add(cb), () => memberCbs.delete(cb)),
    toHost: (msg) => post({ k: 'in', to: hostId, msg }),
    onInput: (cb) => (inputCbs.add(cb), () => inputCbs.delete(cb)),
    publish(key, value) {
      doc.set(key, value);
      post({ k: 'state', key, value });
    },
    onState: (cb) => (stateCbs.add(cb), () => stateCbs.delete(cb)),
    sendTo: (to, msg) => post({ k: 'reply', to, msg }),
    onReply: (cb) => (replyCbs.add(cb), () => replyCbs.delete(cb)),
    close() {
      clearInterval(beat);
      post({ k: 'bye' });
      bc.close();
    },
    participantsApi: { list, on: (cb) => (memberCbs.add(cb), () => memberCbs.delete(cb)) },
  };
}
