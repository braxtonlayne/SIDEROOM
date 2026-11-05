// @ts-check
// Development stand-in for SIDE ROOM. `?user=alice` names the person (several tabs = several people), `?instance=`
// the game (default "local"), `?guest=1` plays the page as a guest (no packs). Tabs of one instance find each other
// over a BroadcastChannel (net/bc-channel.js); the first tab hosts. A pack reference loads `packs/<first 16 hex of
// the hash>.json` from the page's own folder, standing in for a shelf object outside SIDE ROOM.

import { createBroadcastChannel } from '../net/bc-channel.js';

/** @typedef {import('./platform.js').Platform} Platform */

/** @param {Window} win @returns {Platform} */
export function createFakePlatform(win) {
  const q = new URLSearchParams(win.location.search);
  const name = (q.get('user') || `Player ${Math.floor(Math.random() * 90 + 10)}`).slice(0, 20);
  const me = { id: `fake-${name.replace(/[^A-Za-z0-9_-]/g, '_')}`, name };
  const instance = (q.get('instance') || 'local').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 64);
  const guest = q.get('guest') === '1';
  /** @type {Awaited<ReturnType<typeof createBroadcastChannel>> | null} */
  let channel = null;

  return {
    kind: 'fake',
    guest,
    canLoadPacks: !guest,
    get channel() {
      return channel;
    },
    async ready() {
      channel = await createBroadcastChannel({ instance, self: me, win });
    },
    identity: () => me,
    instanceId: () => instance,
    setActivity(text) {
      win.document.title = text ? `Sketch & Slander: ${text}` : 'Sketch & Slander';
    },
    async loadPack(ref) {
      if (guest) throw new Error('guests cannot load packs');
      const res = await fetch(`packs/${ref.object_hash.slice(0, 16)}.json`, { credentials: 'omit' });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return new Uint8Array(await res.arrayBuffer());
    },
  };
}
