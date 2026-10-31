// @ts-check
// SIDE ROOM: the Activity runs at <application id>.activities.<host> in the client's iframe and talks to it
// through the Embedded App SDK (packages/embedded-app-sdk), vendored into the bundle as ./sdk/*.js by
// tools/bundle.mjs. Unlike the standalone game there is no sign-in and no game server: who people are comes
// from the SDK, and the game's data channel (`sdk.state`, `sdk.input`) is the only way members reach each other.

import { channelFromSdk } from '../net/sdk-channel.js';

/** @typedef {import('./platform.js').Platform} Platform */
/** @typedef {import('./platform.js').Identity} Identity */

/** Whether this page runs as a SIDE ROOM Activity. @param {Window} win */
export const isSideRoom = (win) => win.parent !== win && /^[^.]+\.activities\./.test(win.location.hostname);

/** @param {any} u @returns {Identity} */
const person = (u) => ({ id: String(u.id), name: String(u.global_name || u.username || 'Player').slice(0, 24) });

/**
 * Who I am and who launched the instance: the SDK's `sdk.self` and `sdk.hostId` (ACT-CH-001), which the client
 * sets when it attaches the data channel at the handshake.
 * @param {{ self?: string | null, hostId?: string | null }} sdk
 */
export function memberIds(sdk) {
  return { self: sdk.self ? String(sdk.self) : '', host: sdk.hostId ? String(sdk.hostId) : '' };
}

/** @param {Window} win @returns {Platform} */
export function createSideRoomPlatform(win) {
  const applicationId = win.location.hostname.split('.')[0];
  /** @type {any} */
  let sdk = null;
  /** @type {any} */
  let config = null;
  /** @type {Identity} */
  let me = { id: 'sideroom-unknown', name: 'Player' };
  /** @type {import('../net/channel.js').Channel | null} */
  let channel = null;
  /** @type {Identity[]} */
  let people = [];
  let lastActivity = '';
  let lastActivityAt = -Infinity;

  /** @param {any} data */
  const humans = (data) => (data?.participants ?? []).filter((/** @type {any} */ p) => !p.bot).map(person);

  return {
    kind: 'sideroom',
    get channel() {
      return channel;
    },
    async ready() {
      // The SDK is vendored next to the page by tools/bundle.mjs (it is not in the source tree).
      // @ts-ignore
      const mod = await import('../sdk/index.js');
      sdk = new mod.SideRoomSDK(applicationId);
      config = await sdk.ready();
      const ids = memberIds(sdk);
      if (!ids.self) throw new Error('This SIDE ROOM client does not tell activities who they are yet (needs the data channel).');
      people = humans(await sdk.commands.getInstanceConnectedParticipants().catch(() => ({ participants: [] })));
      me = people.find((p) => p.id === ids.self) ?? { id: ids.self, name: 'Player' };
      channel = await channelFromSdk(sdk, {
        self: me,
        hostId: ids.host || (sdk.isHost ? ids.self : ''),
        participants: async () => people,
        onParticipants: (cb) =>
          sdk.subscribe('ACTIVITY_INSTANCE_PARTICIPANTS_UPDATE', (/** @type {any} */ data) => {
            people = humans(data);
            cb(people);
          }),
      });
    },
    identity: () => me,
    ticket: () => null,
    participants: async () => people,
    onParticipantsChange: (cb) => channel?.onMembers(cb) ?? (() => {}),
    instanceId: () => String(config?.instance_id ?? 'sideroom').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 128),
    setActivity(text) {
      if (!sdk || text === lastActivity) return;
      // At most every 15 s: the line follows the land share, which changes all the time.
      if (performance.now() - lastActivityAt < 15_000) return;
      lastActivityAt = performance.now();
      lastActivity = text;
      sdk.commands.setActivity({ activity: { details: 'Playing Land.io', state: text || 'In the lobby' } }).catch(() => {});
    },
  };
}
