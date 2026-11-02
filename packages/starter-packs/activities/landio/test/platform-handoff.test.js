// @ts-check
// D-388 (wave 46): host migration end to end over the REAL Embedded App SDK. The platform half (the server picks
// the longest-connected member and the client bridge sends HOST_CHANGED) is stood in for by calling the SDK's
// `data.changeHost`, which is exactly what the SDK does on HOST_CHANGED. Everything after that is the shipped code:
// the SDK moves the host and re-stamps the host keys, sdk-channel follows it, and transport.js hands it to
// `handoff.hostChanged` with snapshots on (HANDOFF_SNAPSHOT_TICKS), so the game carries on for everyone.
import assert from 'node:assert/strict';
import { LoopbackHub, SideRoomSDK } from '../../../../embedded-app-sdk/src/index.ts';
import { channelFromSdk } from '../client/net/sdk-channel.js';
import { HANDOFF_SNAPSHOT_TICKS, sdkTransportFor } from '../client/net/transport.js';
import { World } from '../client/world.js';

const TICK_MS = 1000 / 7;
const flush = async () => {
  for (let i = 0; i < 4; i++) await new Promise((r) => setTimeout(r, 0));
};
const stubEnv = () => ({ parent: null, hostOrigin: '', hostWindow: null, addEventListener() {}, removeEventListener() {} });

test('the SDK moving the host hands the game to the successor; the other guest keeps playing', async () => {
  const clock = { t: 1000 };
  const hub = new LoopbackHub('h1');
  /** @type {{id: string, name: string}[]} */
  let call = [];
  /** @type {Set<(l: any[]) => void>} */
  const subs = new Set();
  /** @param {string} id @param {string} name */
  async function member(id, name) {
    call = [...call, { id, name }];
    subs.forEach((cb) => cb(call));
    const sdk = new SideRoomSDK('app', { env: /** @type {any} */ (stubEnv()), transport: hub.connect(id) });
    const ch = await channelFromSdk(/** @type {any} */ (sdk), {
      self: { id, name },
      hostId: 'h1',
      participants: () => Promise.resolve(call),
      onParticipants: (cb) => (subs.add(cb), () => subs.delete(cb)),
    });
    const t = sdkTransportFor(ch, { now: () => clock.t, engine: { instance: 'inst-1', cfg: { targetHeads: 0 }, seed: 's' } });
    const world = new World();
    const joined = await t.join({ uid: name, name });
    assert.ok(joined.ok, String(joined.error));
    world.ingest(joined, clock.t);
    /** @type {any[]} */
    const replies = [];
    t.start((/** @type {any} */ r, /** @type {number} */ at) => (replies.push(r), world.ingest(r, at)), () => world.cursor());
    return { id, sdk, ch, t, world, replies };
  }
  const h = await member('h1', 'Alice');
  const g1 = await member('g1', 'Bob');
  const g2 = await member('g2', 'Cy');
  await h.t.play();
  await g1.t.play();
  const play2 = await g2.t.play();
  assert.equal(play2.ok, true);
  /** @param {any} t @param {number} n */
  const run = async (t, n) => {
    for (let i = 0; i < n; i++) {
      clock.t += TICK_MS;
      t.engine?.tick();
      await flush();
    }
  };
  await run(h.t, HANDOFF_SNAPSHOT_TICKS + 5);
  assert.ok(g1.t.handoff.snapshot(), 'snapshots are on: the guests hold the newest one');
  const pid2 = /** @type {any} */ (h.t.engine).byUid.get('g2').pid;

  // The host's page goes away; the platform names g1 (the longest-connected member left) and every page's SDK hears
  // HOST_CHANGED. g2 hears it first.
  /** @type {any} */ (h.t.engine).close();
  h.t.stop();
  hub.disconnect('h1');
  h.sdk.close();
  call = call.filter((m) => m.id !== 'h1');
  subs.forEach((cb) => cb(call));
  g2.sdk.data.changeHost('g1');
  g1.sdk.data.changeHost('g1');
  await flush();
  assert.equal(g1.sdk.isHost, true);
  assert.equal(g1.ch.hostId, 'g1');
  assert.equal(g2.ch.hostId, 'g1');
  const eng = /** @type {any} */ (g1.t.engine);
  assert.ok(eng, 'the successor runs the game now');
  assert.equal(eng.byUid.get('g2').pid, pid2, 'the other guest got its seat and player back');
  await run(g1.t, 6);
  g2.t.input({ turns: [1] });
  await run(g1.t, 4);
  assert.equal(eng.state.players.find((/** @type {any} */ p) => p.id === pid2).dir, 1, "the guest's input reaches the new host");
  assert.equal(g2.world.recvTick, eng.state.tick, 'the guest follows the new host');
  assert.ok(!g2.replies.some((r) => r.error === 'host-left'), 'the guest never saw the game end');
  // A non-host still cannot write the host keys.
  assert.throws(() => g2.sdk.state.set({ 'host:tick': 1 }), /Only the host/);
  await g1.t.leave();
  await g2.t.leave();
});
