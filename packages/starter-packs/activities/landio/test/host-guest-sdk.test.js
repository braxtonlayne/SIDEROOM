// @ts-check
// The same host/guest game over the REAL Embedded App SDK data channel (sdk.state, sdk.input.send / reply,
// sdk.on) on the SDK's own LoopbackHub, cherry-picked from feat/p36-act-channel (ACT-CH-001 first slice). The
// hub plays the SFU: it only ever sees the opaque bytes the SDK sends.
import assert from 'node:assert/strict';
import { LoopbackHub, SideRoomSDK, MAX_DATA_BYTES } from '../../../../embedded-app-sdk/src/index.ts';
import { channelFromSdk } from '../client/net/sdk-channel.js';
import { createSdkTransport } from '../client/net/sdk-transport.js';
import { World } from '../client/world.js';

const TICK_MS = 1000 / 7;
const flush = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};

const stubEnv = () => ({ parent: null, hostOrigin: '', hostWindow: null, addEventListener() {}, removeEventListener() {} });

/** Who is in the call, shared by every member's `participants` provider. */
function roster() {
  /** @type {{id: string, name: string}[]} */
  let list = [];
  /** @type {Set<(l: any[]) => void>} */
  const subs = new Set();
  return {
    add(id, name) {
      list = [...list, { id, name }];
      subs.forEach((cb) => cb(list));
    },
    remove(id) {
      list = list.filter((m) => m.id !== id);
      subs.forEach((cb) => cb(list));
    },
    provider: () => Promise.resolve(list),
    on: (cb) => (subs.add(cb), () => subs.delete(cb)),
  };
}

async function setup(chunkChars) {
  const clock = { t: 1000 };
  const hub = new LoopbackHub('h1');
  const call = roster();
  /** @param {string} id @param {string} name */
  async function member(id, name) {
    call.add(id, name);
    const sdk = new SideRoomSDK('app', { env: stubEnv(), transport: hub.connect(id) });
    const ch = await channelFromSdk(sdk, { self: { id, name }, hostId: 'h1', participants: call.provider, onParticipants: call.on, chunkChars });
    return { sdk, ch, t: createSdkTransport({ channel: ch, now: () => clock.t, engine: { instance: 'inst-1', cfg: { targetHeads: 0 }, seed: 's' } }) };
  }
  const host = await member('h1', 'Alice');
  const run = async (n) => {
    for (let i = 0; i < n; i++) {
      clock.t += TICK_MS;
      host.t.engine?.tick();
      await flush();
    }
  };
  const page = async (m, name) => {
    const world = new World();
    const joined = await m.t.join({ uid: name, name });
    assert.ok(joined.ok, String(joined.error));
    world.ingest(joined, clock.t);
    const replies = [];
    m.t.start((r, at) => (replies.push(r), world.ingest(r, at)), () => world.cursor());
    return { ...m, world, joined, replies };
  };
  return { clock, hub, call, host, member, run, page };
}

test('over the SDK: join, play, input and ack, with every message a small envelope', async () => {
  const s = await setup();
  const hp = await s.page(s.host, 'Alice');
  const g = await s.page(await s.member('g1', 'Bob'), 'Bob');
  assert.ok(g.joined.full);
  assert.equal(g.joined.you.host, false);
  await hp.t.play();
  const pg = await g.t.play();
  assert.equal(pg.ok, true);
  await s.run(15);
  assert.equal(g.world.recvTick, s.host.t.engine.state.tick);
  assert.equal(g.world.contig, s.host.t.engine.state.tick, 'no gaps');
  assert.equal(g.world.roster.size, 2);
  const seq = g.t.input({ turns: [2] });
  await s.run(3);
  assert.ok(g.world.you.ack[0] >= seq);
  assert.equal(s.host.t.engine.state.players.find((p) => p.id === pg.pid).dir, 2);
  for (const r of s.hub.relayed) assert.ok(r.bytes.length <= MAX_DATA_BYTES);
  await hp.t.leave();
});

test('over the SDK: a 200x200 arena stays inside the envelope limit', async () => {
  const s = await setup();
  // Make the arena big so the full snapshot is far over the 16 KiB envelope.
  s.host.t.engine?.close();
  const hostCfg = { w: 200, h: 200, targetHeads: 0 };
  const host2 = createSdkTransport({ channel: s.host.ch, now: () => s.clock.t, engine: { instance: 'big', cfg: hostCfg, seed: 'big' } });
  s.host.t = host2;
  const hp = await s.page(s.host, 'Alice');
  const g = await s.page(await s.member('g1', 'Bob'), 'Bob');
  assert.equal(g.world.w, 200);
  await g.t.play();
  await s.run(6);
  assert.equal(g.world.contig, host2.engine.state.tick);
  const biggest = Math.max(...s.hub.relayed.map((r) => r.bytes.length));
  assert.ok(biggest <= MAX_DATA_BYTES, `largest envelope ${biggest}`);
  await hp.t.leave();
});

test('over the SDK: pieces reassemble when envelopes are tiny (chunk size 300)', async () => {
  const s = await setup(300);
  const hp = await s.page(s.host, 'Alice');
  const g = await s.page(await s.member('g1', 'Bob'), 'Bob');
  await g.t.play();
  await s.run(10);
  assert.equal(g.world.contig, s.host.t.engine.state.tick);
  assert.ok(s.hub.relayed.length > 20, 'many small envelopes: ' + s.hub.relayed.length);
  await hp.t.leave();
});

test('over the SDK: a guest cannot publish host state', async () => {
  const s = await setup();
  await s.page(s.host, 'Alice');
  const g = await s.page(await s.member('g1', 'Bob'), 'Bob');
  assert.throws(() => g.sdk.state.set({ 'host:tick': { now: 999999, recs: [] } }), /Only the host/);
  // A member who is not the host cannot spoof the host's frames through the shared state either.
  assert.equal(g.world.recvTick < 999999, true);
  await s.host.t.leave();
});

test('over the SDK: members leaving the call free their seat; the host leaving ends it for guests', async () => {
  const s = await setup();
  await s.page(s.host, 'Alice');
  const g = await s.page(await s.member('g1', 'Bob'), 'Bob');
  await g.t.play();
  await s.run(2);
  assert.equal(s.host.t.engine.arena.seats.size, 2);
  s.call.remove('g1');
  await flush();
  assert.equal(s.host.t.engine.arena.seats.size, 1);
  const g2 = await s.page(await s.member('g2', 'Cy'), 'Cy');
  g2.replies.length = 0;
  s.call.remove('h1');
  await flush();
  assert.ok(g2.replies.some((r) => r.error === 'host-left'));
  await s.host.t.leave();
});
