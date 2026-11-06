// @ts-check
// SIDE ROOM: the Activity runs at <application id>.activities.<host> in the client's iframe and talks to it through
// the Embedded App SDK (packages/embedded-app-sdk), vendored into the bundle as ./sdk/*.js by tools/bundle.mjs.
// Who people are comes from the SDK; the data channel (`sdk.state`, `sdk.input`) is the only way pages reach each
// other.
//
// D-396: the game reads no chat, Scrapbook or profile. A member's starting nickname is their display name from the
// instance's participant list (never the handle); a guest's is the callsign the lobby gave it. D-397: a guest's page
// sends only GUEST_COMMANDS (test/guest-sdk.test.js holds it to that with an SDK that throws on anything else).

import { channelFromSdk } from '../net/sdk-channel.js';
import { PACK_MAX_BYTES } from '../../shared/pack.js';

/** @typedef {import('./platform.js').Platform} Platform */
/** @typedef {import('./platform.js').Identity} Identity */

/** OPS-TDEP-001: a test deployment serves the Activity from `/_activity/<application id>/` on the main host. */
const ACTIVITY_PATH = /^\/_activity\/(\d{1,20})\//;

/** Whether this page runs as a SIDE ROOM Activity. @param {Window} win */
export const isSideRoom = (win) =>
  win.parent !== win &&
  (/^[^.]+\.activities\./.test(win.location.hostname) || ACTIVITY_PATH.test(win.location.pathname ?? ''));

/** The application id: `<id>` of `/_activity/<id>/` or of `<id>.activities.<host>`. @param {Window} win */
export const applicationIdOf = (win) =>
  ACTIVITY_PATH.exec(win.location.pathname ?? '')?.[1] ?? win.location.hostname.split('.')[0];

/** A member as the game names them: the display name only, never the handle. @param {any} u @returns {Identity} */
export const person = (u) => {
  const display = String(u.global_name ?? '').trim();
  const handle = String(u.username ?? '').trim();
  // The client fills global_name with the handle when a member never set a display name: that is still a handle.
  const name = display && display.toLowerCase() !== handle.toLowerCase() ? display : 'Player';
  return { id: String(u.id), name: name.slice(0, 20) };
};

/** The bridge commands a guest page may send (ARC-GST-002, D-397). Everything else is a member's. */
export const GUEST_COMMANDS = Object.freeze(['ready', 'dataSend', 'subscribe', 'unsubscribe', 'storageSet', 'storageRemove']);
/** The only event a guest page subscribes to: the data channel's own (the SDK does it when it attaches). */
export const GUEST_EVENTS = Object.freeze(['DATA_MESSAGE', 'HOST_CHANGED']);

/** The capability a bundle declares to read one shelf object per ask (D-389). */
export const BYOS_READ = 'byos_read';

/**
 * @param {Window} win
 * @param {{ loadSdk?: () => Promise<any> }} [opt]   tests pass a fake bridge
 * @returns {Platform}
 */
export function createSideRoomPlatform(win, opt = {}) {
  const applicationId = applicationIdOf(win);
  /** @type {any} */
  let sdk = null;
  /** @type {any} */
  let config = null;
  /** @type {Identity} */
  let me = { id: 'sideroom-unknown', name: 'Player' };
  /** @type {import('../net/channel.js').Channel | null} */
  let channel = null;
  let guest = false;
  let lastActivity = '';
  let lastActivityAt = -Infinity;

  /** @param {any} data */
  const humans = (data) => (data?.participants ?? []).filter((/** @type {any} */ p) => !p.bot).map(person);

  return {
    kind: 'sideroom',
    get guest() {
      return guest;
    },
    get canLoadPacks() {
      return !guest && !!config?.capabilities?.includes(BYOS_READ) && typeof sdk?.commands?.shelfObjectBytes === 'function';
    },
    get canPickPacks() {
      return this.canLoadPacks && typeof sdk?.commands?.pickShelfObject === 'function';
    },
    get channel() {
      return channel;
    },
    async ready() {
      // @ts-ignore the SDK is vendored next to the page by tools/bundle.mjs (it is not in the source tree)
      const mod = await (opt.loadSdk ? opt.loadSdk() : import('../sdk/index.js'));
      sdk = new mod.SideRoomSDK(applicationId);
      config = await sdk.ready();
      // The sandboxed frame has no localStorage of its own (ARC-SBX-001): the nickname and settings live in the
      // host's per-member store, which the SDK serves through window.localStorage from here on.
      if (config.capabilities?.includes('storage')) sdk.storage.install(win);
      const self = sdk.self ? String(sdk.self) : '';
      const host = sdk.hostId ? String(sdk.hostId) : '';
      if (!self) throw new Error('This SIDE ROOM client does not attach the data channel.');
      guest = config.guest === true;
      /** @type {Identity[]} */
      let people;
      if (guest) {
        me = { id: self, name: String(config.callsign || 'Guest').replace(/[\p{C}<>]/gu, '').trim().slice(0, 20) || 'Guest' };
        people = [me];
      } else {
        people = humans(await sdk.commands.getInstanceConnectedParticipants().catch(() => ({ participants: [] })));
        me = people.find((p) => p.id === self) ?? { id: self, name: 'Player' };
      }
      channel = await channelFromSdk(sdk, {
        self: me,
        hostId: host || (sdk.isHost ? self : ''),
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
    instanceId: () => String(config?.instance_id ?? 'sideroom').replace(/[^A-Za-z0-9_.:-]/g, '_').slice(0, 128),
    setActivity(text) {
      if (!sdk || guest || text === lastActivity) return;
      if (performance.now() - lastActivityAt < 15_000) return;
      lastActivityAt = performance.now();
      lastActivity = text;
      sdk.commands.setActivity({ activity: { details: 'Playing Sketch & Slander', state: text || 'In the lobby' } }).catch(() => {});
    },
    async pickPack() {
      if (!this.canPickPacks) throw new Error('the file picker needs a member and the byos_read capability');
      // D-466: the member's page opens the app's own shelf picker (never inside this frame) and answers with the one
      // object they chose, or null. The choice is their yes: the shelfObjectBytes that follows asks nothing more.
      const pick = await sdk.commands.pickShelfObject({ accept: ['.json', '.txt'], max_size: PACK_MAX_BYTES });
      if (!pick) return null;
      return { shelf_id: pick.shelf_id, object_hash: pick.object_hash, name: String(pick.name ?? '') };
    },
    async loadPack(ref) {
      if (!this.canLoadPacks) throw new Error('packs need a member and the byos_read capability');
      // D-389 / D-460: the member's page mints the per-object URL for this one object (after the member says yes),
      // fetches it and hands this frame the bytes; the game's CSP never names the storage host.
      const bytes = await sdk.commands.shelfObjectBytes({ shelf_id: ref.shelf_id, object_hash: ref.object_hash });
      if (!(bytes instanceof Uint8Array)) throw new Error('not-bytes');
      if (bytes.length > PACK_MAX_BYTES) throw new Error('too-big');
      return bytes;
    },
  };
}
