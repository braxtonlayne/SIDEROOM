// @ts-check
// The guest path (a /play/<token> link, ReadyConfig.guest): a guest's bridge has a reduced command set, so the
// platform and the game must use only the data channel, `ready` and `storage`. Two proofs:
//   1. the real SideRoomSDK behind a bridge that records every command it is sent: a guest plays a whole join
//      and a few seconds of movement against a host, and no member-only command is ever sent;
//   2. an SDK whose every `commands.*` throws: the guest platform still comes up.
import assert from 'node:assert/strict';
import initJolt from 'jolt-physics/wasm-compat';
import { LoopbackHub, SideRoomSDK, PROTOCOL_VERSION } from '../../../../embedded-app-sdk/src/index.ts';
import { createSideRoomPlatform, MEMBER_ONLY_COMMANDS } from '../platform/sideroom-platform.js';
import { channelFromSdk } from '../net/sdk-channel.js';
import { HostSession } from '../net/host.js';
import { PredictedClient } from '../net/client.js';
import { World } from '../sim/world.js';
import { BTN } from '../sim/input.js';
import { loadSettings, saveSettings } from '../client/settings.js';

const GUEST_SAFE = new Set(['storageSet', 'storageRemove', 'subscribe', 'unsubscribe', 'dataSend']);
const ORIGIN = 'https://sideroom.example';

/**
 * A fake client page for one SDK: answers the handshake with `config` and every request with ok, and records
 * the command names.
 * @param {any} config
 */
function bridge(config) {
  /** @type {string[]} */
  const commands = [];
  /** @type {((e: any) => void)[]} */
  const listeners = [];
  const hostWindow = {};
  /** @param {any} data */
  const send = (data) => queueMicrotask(() => listeners.forEach((l) => l({ source: hostWindow, origin: ORIGIN, data })));
  const env = {
    hostOrigin: ORIGIN,
    hostWindow,
    addEventListener: (/** @type {string} */ _t, /** @type {(e: any) => void} */ l) => void listeners.push(l),
    removeEventListener() {},
    parent: {
      /** @param {any} frame */
      postMessage(frame) {
        if (frame.kind === 'handshake') send({ sr: PROTOCOL_VERSION, kind: 'ready', config });
        else if (frame.kind === 'request') {
          commands.push(frame.cmd);
          send({ sr: PROTOCOL_VERSION, kind: 'response', nonce: frame.nonce, ok: true, data: { participants: [] } });
        }
      },
    },
  };
  return { env, commands };
}

/** The window an Activity sees inside SIDE ROOM. */
const win = /** @type {any} */ ({ location: { hostname: 'app123.activities.sideroom.example', search: '' }, parent: {} });

const flush = async () => {
  for (let i = 0; i < 6; i++) await Promise.resolve();
};

test('a guest joins and plays through the real SDK without sending one member-only command', async () => {
  const J = await initJolt();
  const hub = new LoopbackHub('m-host');
  // The host is a member (its bridge is not under test here).
  const hostSdk = new SideRoomSDK('app123', { env: bridge({}).env, transport: hub.connect('m-host') });
  const host = new HostSession({ channel: channelFromSdk(hostSdk, { name: 'Host' }), world: new World(J, { seed: 5 }), name: 'Host' });

  const guestConfig = {
    application_id: 'app123', instance_id: 'i-1', channel_id: '0', guild_id: '0', guest: true, callsign: 'Fox',
    user_id: 'lobby-7', host_id: 'm-host', capabilities: ['datachannel', 'storage', 'guest_play'], storage: {},
  };
  const b = bridge(guestConfig);
  const guestSdk = new SideRoomSDK('app123', { env: b.env, transport: hub.connect('lobby-7') });
  const platform = createSideRoomPlatform(win, { createSdk: async () => guestSdk });
  await platform.ready();
  assert.equal(platform.guest, true);
  assert.equal(platform.identity().name, 'Fox', 'a guest plays under the callsign the relay gives');
  assert.ok(platform.channel && !platform.channel.isHost);

  // Settings go through the storage capability.
  const s = loadSettings(platform.storage);
  assert.ok(saveSettings(platform.storage, { ...s, sensitivity: 9 }));
  assert.equal(loadSettings(platform.storage).sensitivity, 9);

  const client = new PredictedClient({ channel: platform.channel, world: new World(J, { seed: 0 }), name: platform.identity().name });
  for (let i = 0; i < 240; i++) {
    host.tick({ buttons: 0, yaw: 0, pitch: 0 });
    client.tick({ buttons: i > 30 ? BTN.FORWARD : 0, yaw: 4000, pitch: 0 });
    await flush();
  }
  assert.equal(client.status, 'playing');
  assert.ok(client.slot > 0);
  const e = host.world.eye(client.slot);
  assert.ok(e && Math.hypot(e.x - host.world.level.spawns[client.slot][0], e.z - host.world.level.spawns[client.slot][2]) > 2, 'the guest moved on the host');
  assert.deepEqual(host.roster().map((r) => r.name), ['Host', 'Fox']);
  platform.setActivity('In the arena'); // a no-op for guests

  assert.ok(b.commands.includes('storageSet'), 'storage was used');
  for (const cmd of b.commands) assert.ok(GUEST_SAFE.has(cmd), `the guest path sent ${cmd}`);
  for (const cmd of MEMBER_ONLY_COMMANDS) assert.ok(!b.commands.includes(cmd), cmd);
});

test('a guest platform comes up even when every SDK command throws', async () => {
  const hub = new LoopbackHub('m-host');
  const b = bridge({ application_id: 'a', instance_id: 'i', channel_id: '0', guild_id: '0', guest: true, callsign: 'Owl', capabilities: ['datachannel', 'storage', 'guest_play'] });
  const sdk = new SideRoomSDK('a', { env: b.env, transport: hub.connect('lobby-1') });
  /** @type {string[]} */
  const tried = [];
  Object.defineProperty(sdk, 'commands', {
    value: new Proxy({}, { get: (_t, name) => () => { tried.push(String(name)); throw new Error(`${String(name)} is member-only`); } }),
  });
  const platform = createSideRoomPlatform(win, { createSdk: async () => sdk });
  await platform.ready();
  platform.setActivity('x');
  assert.deepEqual(tried, []);
  assert.equal(platform.identity().name, 'Owl');
});

test('a member (not a guest) may use the participant list for their name', async () => {
  const hub = new LoopbackHub('m-1');
  const b = bridge({ application_id: 'a', instance_id: 'i', channel_id: 'c', guild_id: 'g', user_id: 'm-1', host_id: 'm-1', capabilities: ['datachannel', 'storage'] });
  const sdk = new SideRoomSDK('a', { env: b.env, transport: hub.connect('m-1') });
  Object.defineProperty(sdk, 'commands', {
    value: { getInstanceConnectedParticipants: async () => ({ participants: [{ id: 'm-1', username: 'ada', global_name: 'Ada' }] }), setActivity: async () => null },
  });
  const platform = createSideRoomPlatform(win, { createSdk: async () => sdk });
  await platform.ready();
  assert.equal(platform.guest, false);
  assert.equal(platform.identity().name, 'Ada');
  assert.ok(platform.channel?.isHost);
});
