// @ts-check
// Development stand-in for SIDE ROOM. `?user=alice` names the player (several tabs = several players) and
// `?instance=room1` picks the room (default "local"). Tabs of one room find each other over a
// BroadcastChannel (net/bc-channel.js); the first tab hosts, as the member who launches an Activity does.

import { createBroadcastChannel } from '../net/bc-channel.js';
import { memoryStore } from './platform.js';

/** @typedef {import('./platform.js').Platform} Platform */

/** @param {Window} win @returns {Platform} */
export function createFakePlatform(win) {
  const q = new URLSearchParams(win.location.search);
  /** @type {import('./platform.js').KeyValue} */
  let storage;
  try {
    storage = win.localStorage;
    storage.getItem('probe');
  } catch {
    storage = memoryStore();
  }
  // Math.random only names a dev tab; the simulation never sees it.
  const name = (q.get('user') || storage.getItem('arena3d.name') || `player${Math.floor(Math.random() * 900 + 100)}`).slice(0, 24);
  const me = { id: `fake-${name.replace(/[^A-Za-z0-9_-]/g, '_')}-${Math.floor(Math.random() * 1e6)}`, name };
  const instance = (q.get('instance') || 'local').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 64);
  /** @type {import('../net/channel.js').Channel | null} */
  let channel = null;

  return {
    kind: 'fake',
    guest: q.get('guest') === '1',
    get channel() {
      return channel;
    },
    storage,
    async ready() {
      channel = await createBroadcastChannel({ instance, self: me, win });
    },
    identity: () => me,
    instanceId: () => instance,
    setActivity(text) {
      win.document.title = text ? `Arena 3D: ${text}` : 'Arena 3D';
    },
    close() {
      channel?.close();
    },
  };
}
