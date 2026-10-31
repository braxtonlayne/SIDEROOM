// @ts-check
// Development stand-in for SIDE ROOM. `?user=alice` names the person (several tabs = several people) and
// `?instance=room1` the Activity instance (default "local"). Tabs of one instance find each other over a
// BroadcastChannel (net/bc-channel.js), which plays the part of the voice channel's participant list and of the
// SDK's data channel: the first tab hosts the game, as the member who launched an activity does in SIDE ROOM.

import { createBroadcastChannel } from '../net/bc-channel.js';

/** @typedef {import('./platform.js').Platform} Platform */
/** @typedef {import('./platform.js').Identity} Identity */

/** @param {Window} win @returns {Platform} */
export function createFakePlatform(win) {
  const q = new URLSearchParams(win.location.search);
  const name = (q.get('user') || `guest${Math.floor(Math.random() * 900 + 100)}`).slice(0, 24);
  const me = { id: `fake-${name.replace(/[^A-Za-z0-9_-]/g, '_')}`, name };
  const instance = (q.get('instance') || 'local').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 64);
  /** @type {Awaited<ReturnType<typeof createBroadcastChannel>> | null} */
  let channel = null;

  return {
    kind: 'fake',
    get channel() {
      return channel;
    },
    async ready() {
      channel = await createBroadcastChannel({ instance, self: me, win });
    },
    identity: () => me,
    ticket: () => null,
    participants: async () => channel?.members() ?? [me],
    onParticipantsChange: (cb) => channel?.onMembers(cb) ?? (() => {}),
    instanceId: () => instance,
    setActivity(text) {
      win.document.title = text ? `Land.io — ${text}` : 'Land.io';
    },
  };
}
