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

/**
 * The bridge commands a guest page may send (ARC-GST-002, D-397): a guest on a `/play/<token>` link has no channel,
 * server or member data, so its bridge answers only the handshake, the data channel and storage. Everything else
 * (authorize, authenticate, getChannel, the participant list, Rich Presence) is a member's and is never sent on
 * the guest path. test/host-guest-sdk.test.js holds the page to this list.
 */
export const GUEST_COMMANDS = Object.freeze(['ready', 'dataSend', 'subscribe', 'unsubscribe', 'storageSet', 'storageRemove']);
/** The only event a guest page subscribes to: the data channel's own (the SDK does it when it attaches). */
export const GUEST_EVENTS = Object.freeze(['DATA_MESSAGE']);

/**
 * @param {Window} win
 * @param {{ loadSdk?: () => Promise<any> }} [opt]   loadSdk: where the SDK comes from (tests pass a fake bridge)
 * @returns {Platform}
 */
export function createSideRoomPlatform(win, opt = {}) {
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
  /** A guest on a `/play/<token>` link: only the commands in GUEST_COMMANDS. */
  let guest = false;
  let lastActivityAt = -Infinity;

  /** @param {any} data */
  const humans = (data) => (data?.participants ?? []).filter((/** @type {any} */ p) => !p.bot).map(person);

  return {
    kind: 'sideroom',
    get guest() {
      return guest;
    },
    get channel() {
      return channel;
    },
    async ready() {
      // The SDK is vendored next to the page by tools/bundle.mjs (it is not in the source tree).
      // @ts-ignore
      const mod = await (opt.loadSdk ? opt.loadSdk() : import('../sdk/index.js'));
      sdk = new mod.SideRoomSDK(applicationId);
      config = await sdk.ready();
      // The sandboxed frame has no localStorage of its own (ARC-SBX-001): settings, key bindings and seen tips
      // live in the host's per-member store, which the SDK serves through window.localStorage from here on.
      if (config.capabilities?.includes('storage')) sdk.storage.install(win);
      const ids = memberIds(sdk);
      if (!ids.self) throw new Error('This SIDE ROOM client does not tell activities who they are yet (needs the data channel).');
      guest = config.guest === true;
      if (guest) {
        // A guest plays under the callsign the lobby gave it and never asks who else is here: the host's frames
        // carry the roster (names of the people playing), which is all a guest may see.
        me = { id: ids.self, name: String(config.callsign || 'Guest').replace(/[\p{C}<>]/gu, '').trim().slice(0, 24) || 'Guest' };
        people = [me];
      } else {
        people = humans(await sdk.commands.getInstanceConnectedParticipants().catch(() => ({ participants: [] })));
        me = people.find((p) => p.id === ids.self) ?? { id: ids.self, name: 'Player' };
      }
      channel = await channelFromSdk(sdk, {
        self: me,
        hostId: ids.host || (sdk.isHost ? ids.self : ''),
        participants: async () => people,
        onParticipants: guest
          ? () => () => {}
          : (cb) =>
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
      // Rich Presence is a member's: a guest has none.
      if (!sdk || guest || text === lastActivity) return;
      // At most every 15 s: the line follows the land share, which changes all the time.
      if (performance.now() - lastActivityAt < 15_000) return;
      lastActivityAt = performance.now();
      lastActivity = text;
      sdk.commands.setActivity({ activity: { details: 'Playing Land.io', state: text || 'In the lobby' } }).catch(() => {});
    },
  };
}
