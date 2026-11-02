// @ts-check
// SIDE ROOM: the Activity runs at <application id>.activities.<host> in the client's iframe and talks to it only
// through the Embedded App SDK, vendored into the bundle as ../sdk/*.js by tools/bundle.mjs.
//
// Guests (ReadyConfig.guest, a /play/<token> link) get a reduced bridge: only the data channel, `ready` and
// `storage` are safe. So this file never calls `authorize`, `authenticate` or `getChannel`, and calls the
// member-only `getInstanceConnectedParticipants` and `setActivity` only when `guest` is false. The game itself
// needs nothing but the data channel: the host learns who plays from HELLO messages (net/host.js), not from
// the platform. test/guest.test.js holds this to account with a bridge that records every command.

import { channelFromSdk } from '../net/sdk-channel.js';
import { memoryStore } from './platform.js';

/** @typedef {import('./platform.js').Platform} Platform */

/** Whether this page runs as a SIDE ROOM Activity. @param {Window} win */
export const isSideRoom = (win) =>
  win.parent !== win &&
  (/^[^.]+\.activities\./.test(win.location.hostname) || ACTIVITY_PATH.test(win.location.pathname ?? ''));

/** OPS-TDEP-001: a test deployment serves the Activity from `/_activity/<application id>/` on the main host. */
const ACTIVITY_PATH = /^\/_activity\/(\d{1,20})\//;

/** The application id: `<id>` of `/_activity/<id>/` (a test deployment) or of `<id>.activities.<host>`. @param {Window} win */
export const applicationIdOf = (win) =>
  ACTIVITY_PATH.exec(win.location.pathname ?? '')?.[1] ?? win.location.hostname.split('.')[0];

/** Commands a guest's bridge may refuse; never sent on the guest path. */
export const MEMBER_ONLY_COMMANDS = Object.freeze(['authorize', 'authenticate', 'getChannel', 'getInstanceConnectedParticipants', 'setActivity', 'openExternalLink']);

/**
 * @param {Window} win
 * @param {{ createSdk?: (applicationId: string) => Promise<any> }} [opt] test seam: build the SDK
 * @returns {Platform}
 */
export function createSideRoomPlatform(win, opt = {}) {
  const applicationId = applicationIdOf(win);
  /** @type {any} */
  let sdk = null;
  /** @type {any} */
  let config = null;
  let me = { id: '', name: 'Player' };
  let storage = memoryStore();
  /** @type {import('../net/channel.js').Channel | null} */
  let channel = null;
  let lastActivity = '';
  let lastActivityAt = -Infinity;

  const createSdk =
    opt.createSdk ??
    (async (/** @type {string} */ id) => {
      // @ts-ignore: the SDK is vendored next to the page by tools/bundle.mjs (not in the source tree)
      const mod = await import('../sdk/index.js');
      return new mod.SideRoomSDK(id);
    });

  /** @type {Platform} */
  const platform = {
    kind: 'sideroom',
    guest: false,
    get channel() {
      return channel;
    },
    get storage() {
      return storage;
    },
    async ready() {
      sdk = await createSdk(applicationId);
      config = await sdk.ready();
      platform.guest = !!config.guest;
      if (config.capabilities?.includes('storage')) storage = sdk.storage;
      if (!sdk.self) throw new Error('This SIDE ROOM client does not give activities a data channel yet.');
      const stored = storage.getItem('arena3d.name');
      let name = platform.guest ? String(config.callsign || 'Guest') : stored || 'Player';
      if (!platform.guest && !stored) {
        // Members only: the participant list says what this member is called.
        try {
          const res = await sdk.commands.getInstanceConnectedParticipants();
          const mine = (res?.participants ?? []).find((/** @type {any} */ p) => String(p.id) === String(sdk.self));
          if (mine) name = String(mine.global_name || mine.username || name);
        } catch {
          /* keep the default */
        }
      }
      me = { id: String(sdk.self), name: name.slice(0, 24) };
      channel = channelFromSdk(sdk, { name: me.name });
    },
    identity: () => me,
    instanceId: () => String(config?.instance_id ?? 'sideroom').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 128),
    setActivity(text) {
      if (!sdk || platform.guest || text === lastActivity) return;
      if (performance.now() - lastActivityAt < 15_000) return;
      lastActivityAt = performance.now();
      lastActivity = text;
      sdk.commands.setActivity({ activity: { details: 'Playing Arena 3D', state: text || 'In the menu' } }).catch(() => {});
    },
    close() {
      channel?.close();
      sdk?.close?.();
    },
  };
  return platform;
}
