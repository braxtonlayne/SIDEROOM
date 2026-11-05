// @ts-check
// A member host and a guest phone over the REAL Embedded App SDK data channel (sdk.state, sdk.input) on the SDK's
// LoopbackHub, which plays the SFU and the guest relay. The guest's SDK is the reduced guest bridge (D-397): the
// handshake, the data channel and storage; every member-only command throws and is recorded, so the test proves the
// game never sends one on the guest path. The member's pack load goes through `shelfObjectBytes` and nothing else.
import assert from 'node:assert/strict';
import { LoopbackHub, MAX_DATA_BYTES, SideRoomSDK } from '../../../../embedded-app-sdk/src/index.ts';
import { channelFromSdk } from '../client/net/sdk-channel.js';
import { createSession } from '../client/net/session.js';
import { GUEST_COMMANDS, GUEST_EVENTS, createSideRoomPlatform } from '../client/platform/sideroom-platform.js';
import { parsePack } from '../shared/pack.js';

const flush = async (n = 6) => {
  for (let i = 0; i < n; i++) await new Promise((r) => setTimeout(r, 0));
};
const stubEnv = () => ({ parent: null, hostOrigin: '', hostWindow: null, addEventListener() {}, removeEventListener() {} });
const win = () => /** @type {any} */ ({ location: { hostname: '77.activities.example.test', pathname: '/' }, parent: {} });

/**
 * The guest bridge as an SDK: ready (guest, callsign, the capabilities cut to datachannel), the data channel, and
 * nothing else. @param {LoopbackHub} hub @param {string} id @param {string[]} log
 */
function guestSdk(hub, id, log) {
  const inner = new SideRoomSDK('77', { env: stubEnv(), transport: hub.connect(id) });
  const commands = new Proxy(
    {},
    {
      get(_t, name) {
        return () => {
          log.push(`command:${String(name)}`);
          throw new Error(`${String(name)} is member-only: refused on the guest bridge`);
        };
      },
    },
  );
  class GuestSDK {
    constructor() {
      this.commands = commands;
      this.state = inner.state;
      this.input = inner.input;
      this.storage = {
        install() {
          log.push('storage:install');
        },
      };
    }
    get self() {
      return inner.self;
    }
    get hostId() {
      return inner.hostId;
    }
    get isHost() {
      return inner.isHost;
    }
    async ready() {
      log.push('ready');
      return { guest: true, callsign: 'Swift Otter', capabilities: ['datachannel'], instance_id: 'i1' };
    }
    /** @param {any} event @param {any} cb */
    on(event, cb) {
      return inner.on(event, cb);
    }
    /** @param {string} event */
    subscribe(event) {
      log.push(`subscribe:${event}`);
      if (!GUEST_EVENTS.includes(event)) throw new Error(`${event} is member-only`);
      return () => {};
    }
  }
  return { SideRoomSDK: GuestSDK };
}

async function setup() {
  const hub = new LoopbackHub('h1');
  const sdk = new SideRoomSDK('77', { env: stubEnv(), transport: hub.connect('h1') });
  const ch = await channelFromSdk(sdk, {
    self: { id: 'h1', name: 'Alice' },
    hostId: 'h1',
    participants: async () => [{ id: 'h1', name: 'Alice' }],
    onParticipants: () => () => {},
  });
  const host = createSession({ channel: ch, name: 'Alice', engine: { instance: 'i1' } });
  host.hello('Alice');
  /** @type {string[]} */
  const log = [];
  const platform = createSideRoomPlatform(win(), { loadSdk: async () => guestSdk(hub, 'gst-1', log) });
  await platform.ready();
  const guest = createSession({ channel: /** @type {any} */ (platform.channel), name: platform.identity().name, guest: platform.guest });
  guest.hello(platform.identity().name);
  await flush();
  return { hub, host, guest, platform, log };
}

test('a guest phone plays a round with a member host using only the guest command set', async () => {
  const { hub, host, guest, platform, log } = await setup();
  try {
    assert.equal(platform.guest, true);
    assert.deepEqual(platform.identity(), { id: 'gst-1', name: 'Swift Otter' });
    assert.equal(platform.canLoadPacks, false);
    await assert.rejects(platform.loadPack({ shelf_id: '1', object_hash: 'a'.repeat(64) }));
    platform.setActivity('Round 1');

    const st = guest.state();
    assert.ok(st);
    assert.deepEqual(st.players.map((p) => [p.name, p.guest]), [['Alice', false], ['Swift Otter', true]]);
    // The host starts; whoever draws, the other guesses the prompt the drawer was told.
    host.send({ t: 'start' });
    await flush();
    const drawerId = host.state()?.drawer;
    const drawer = drawerId === 'h1' ? host : guest;
    const guesser = drawerId === 'h1' ? guest : host;
    assert.equal(drawer.secret().choices.length, 3);
    assert.equal(guesser.secret().choices.length, 0, 'the guesser never sees the choices');
    drawer.send({ t: 'pick', i: 0 });
    await flush();
    const prompt = drawer.secret().prompt;
    assert.ok(prompt);
    drawer.draw({ k: 'p', id: 5, c: 3, w: 1, pts: [10, 10, 200, 150, 400, 300] });
    await flush();
    await new Promise((r) => setTimeout(r, 120));
    host.engine?.tick();
    await flush();
    assert.equal(guesser.ink().length, 1, 'the ink reached the other page');
    assert.equal(guesser.state()?.mask.includes('_'), true);
    guesser.send({ t: 'guess', text: prompt });
    await flush();
    const after = host.state();
    assert.equal(after?.phase, 'reveal');
    assert.equal(after?.word, prompt);
    assert.ok((guest.state()?.players.find((p) => p.id === 'gst-1')?.score ?? 0) > 0);

    // D-397: the guest page sent only the handshake and the data channel; no member command, no other event.
    const commands = log.filter((l) => l.startsWith('command:'));
    assert.deepEqual(commands, [], `member-only commands on the guest path: ${commands}`);
    for (const l of log.filter((x) => x.startsWith('subscribe:'))) assert.ok(GUEST_EVENTS.includes(l.slice(10)), l);
    assert.ok(GUEST_COMMANDS.includes('dataSend'));
    for (const r of hub.relayed) assert.ok(r.bytes.length <= MAX_DATA_BYTES);
    // What the guest's page heard never names a member handle or anything but the game.
    const heard = JSON.stringify(guest.state());
    assert.ok(!/username|handle|guild|channel/i.test(heard), heard);
  } finally {
    guest.leave();
    host.leave();
  }
});

test("a member host's pack loads through shelfObjectBytes only, and its prompts stay in the host page", async () => {
  const pack = new TextEncoder().encode(JSON.stringify({ name: 'Office', prompts: ['stapler duel', 'reply all storm', 'printer jam', 'coffee queue', 'desk plant'] }));
  /** @type {string[]} */
  const calls = [];
  const hub = new LoopbackHub('m1');
  const inner = new SideRoomSDK('77', { env: stubEnv(), transport: hub.connect('m1') });
  const fakeMember = {
    SideRoomSDK: class {
      constructor() {
        this.state = inner.state;
        this.input = inner.input;
        this.storage = { install() {} };
        this.commands = {
          getInstanceConnectedParticipants: async () => ({ participants: [{ id: 'm1', username: 'alice_handle', global_name: 'Alice' }] }),
          shelfObjectBytes: async (/** @type {any} */ ref) => (calls.push(`bytes:${ref.shelf_id}/${ref.object_hash}`), pack),
          shelfObjectUrl: async () => (calls.push('url'), { url: 'https://bucket.example/x' }),
          setActivity: async () => null,
        };
      }
      get self() {
        return inner.self;
      }
      get hostId() {
        return inner.hostId;
      }
      get isHost() {
        return inner.isHost;
      }
      async ready() {
        return { capabilities: ['datachannel', 'byos_read', 'storage'], instance_id: 'i1' };
      }
      on(/** @type {any} */ e, /** @type {any} */ cb) {
        return inner.on(e, cb);
      }
      subscribe() {
        return () => {};
      }
    },
  };
  const realFetch = globalThis.fetch;
  globalThis.fetch = /** @type {any} */ (async () => {
    throw new Error('the game fetched something itself');
  });
  try {
    const platform = createSideRoomPlatform(win(), { loadSdk: async () => fakeMember });
    await platform.ready();
    assert.deepEqual(platform.identity(), { id: 'm1', name: 'Alice' }, 'the display name, never the handle');
    assert.equal(platform.canLoadPacks, true);
    const bytes = await platform.loadPack({ shelf_id: '42', object_hash: 'c'.repeat(64) });
    assert.deepEqual(calls, [`bytes:42/${'c'.repeat(64)}`]);
    const parsed = parsePack(bytes);
    assert.ok(parsed.ok);
    const s = createSession({ channel: /** @type {any} */ (platform.channel), name: 'Alice' });
    if (parsed.ok) s.engine?.addPack(parsed);
    await flush();
    const published = JSON.stringify(inner.state.all());
    assert.ok(published.includes('"Office"'));
    assert.ok(!published.includes('stapler duel'), 'pack prompts are not in the shared state');
    s.leave();
  } finally {
    globalThis.fetch = realFetch;
  }
});
