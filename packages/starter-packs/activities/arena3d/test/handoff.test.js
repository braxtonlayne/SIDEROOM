// @ts-check
// Host migration (D-388, `host_handoff`) over the in-process loopback hub: the host's page goes away, the platform
// names a peer, and the match carries on from the last `host:snap` with every other player in the same slot. Same
// shape as Land.io's test/handoff.test.js; the Game wiring is `handoffRole` (net/handoff.js), which client/game.js
// calls on `channel.onHostChange`.
import assert from 'node:assert/strict';
import initJolt from 'jolt-physics/wasm-compat';
import { HostSession } from '../net/host.js';
import { PredictedClient } from '../net/client.js';
import { handoffRole } from '../net/handoff.js';
import { createLoopbackHub } from '../net/loopback.js';
import { channelFromSdk } from '../net/sdk-channel.js';
import { decodeSeats, encodeSeats } from '../net/seats.js';
import { World } from '../sim/world.js';
import { BTN, idleInput } from '../sim/input.js';

const TICK_MS = 1000 / 60;

/** @param {{ latencyMs?: number, jitterMs?: number, drop?: { input?: number, state?: number, reply?: number } }} [net] */
async function setup(net = {}) {
  const J = await initJolt();
  const hub = createLoopbackHub({ hostId: 'h', latencyMs: net.latencyMs ?? 30, jitterMs: net.jitterMs ?? 0, drop: net.drop, seed: 7 });
  /** @type {{ id: string, channel: import('../net/channel.js').Channel, world: World, host: HostSession | null, client: PredictedClient | null, name: string, gone?: boolean }[]} */
  const members = [];
  /** @param {string} id @param {string} name */
  const add = (id, name) => {
    const channel = hub.join(id, name);
    const world = new World(J, { seed: id === 'h' ? 1234 : 0 });
    const m = {
      id, name, channel, world,
      host: id === 'h' ? new HostSession({ channel, world, name }) : null,
      client: id === 'h' ? null : new PredictedClient({ channel, world, name }),
    };
    members.push(m);
    return m;
  };
  const h = add('h', 'Host');
  const a = add('a', 'Ann');
  const b = add('b', 'Bo');
  let t = 0;
  /** @param {number} ticks @param {Record<string, (i: number) => import('../sim/input.js').PlayerInput>} [inputs] */
  const run = (ticks, inputs = {}) => {
    for (let i = 0; i < ticks; i++) {
      t += TICK_MS;
      hub.pump(t);
      for (const m of members) {
        if (m.gone) continue;
        const input = inputs[m.id]?.(i) ?? idleInput();
        m.host?.tick(input);
        m.client?.tick(input);
        m.client?.frame();
      }
    }
  };
  // What the page does when the old host's page closes and the platform names `next`.
  /** @param {string} next */
  const moveHost = (next) => {
    h.gone = true;
    h.host?.close();
    hub.disconnect('h');
    hub.changeHost(next);
    for (const m of members) {
      if (m.gone) continue;
      const r = handoffRole({ channel: m.channel, world: m.world, name: m.name, host: m.host, client: m.client });
      m.host = r.host;
      m.client = r.client;
    }
  };
  return { J, hub, h, a, b, run, moveHost };
}

/** @param {World} world @param {number} slot */
const eye = (world, slot) => {
  const e = world.eye(slot);
  assert.ok(e, `slot ${slot} exists`);
  return e;
};

test('the seat table round-trips and refuses nonsense', () => {
  const t = { host: 2, seats: [{ id: 'a', slot: 0, name: 'Ann' }, { id: 'b', slot: 1, name: 'Bo' }] };
  assert.deepEqual(decodeSeats(encodeSeats(t)), t);
  const enc = new TextEncoder();
  assert.equal(decodeSeats(enc.encode('not json')), null);
  assert.equal(decodeSeats(enc.encode(JSON.stringify({ v: 2, host: 0, seats: [] }))), null);
  assert.equal(decodeSeats(enc.encode(JSON.stringify({ v: 1, host: 9, seats: [] }))), null, 'slot out of range');
  assert.equal(decodeSeats(enc.encode(JSON.stringify({ v: 1, host: 0, seats: [{ id: 'a', slot: 0, name: 'x' }] }))), null, 'two members in one slot');
  assert.equal(decodeSeats(new Uint8Array(5000)), null, 'oversized');
});

test('the host publishes its seats next to the roster', async () => {
  const s = await setup();
  s.run(90);
  assert.deepEqual(s.a.client?.seatTable?.host, 0);
  const ids = s.a.client?.seatTable?.seats.map((x) => x.id).sort();
  assert.deepEqual(ids, ['a', 'b']);
  const slotOf = (/** @type {string} */ id) => s.a.client?.seatTable?.seats.find((x) => x.id === id)?.slot;
  assert.equal(slotOf('a'), s.a.client?.slot);
  assert.equal(slotOf('b'), s.b.client?.slot);
});

test('the host leaves: the platform names a peer, the match resumes from the last snapshot with the same slots', async () => {
  const s = await setup();
  s.run(120, { a: () => ({ buttons: BTN.FORWARD, yaw: 0, pitch: 0 }) });
  const slotA = /** @type {number} */ (s.a.client?.slot);
  const slotB = /** @type {number} */ (s.b.client?.slot);
  assert.ok(slotA > 0 && slotB > 0 && slotA !== slotB);
  const oldTick = /** @type {HostSession} */ (s.h.host).world.tick;
  const aBefore = eye(/** @type {HostSession} */ (s.h.host).world, slotA);
  const boxesBefore = /** @type {HostSession} */ (s.h.host).world.snapshot().boxes.length;

  s.moveHost('a');
  assert.ok(s.a.host, 'Ann hosts now');
  assert.equal(s.a.client, null);
  assert.ok(s.b.client && !s.b.host, 'Bo stays a peer and says hello again');
  const host = /** @type {HostSession} */ (s.a.host);
  assert.equal(host.selfSlot, slotA, 'Ann keeps her slot');
  assert.ok(host.world.tick <= oldTick && host.world.tick >= oldTick - 12, `resumed from a recent snapshot: ${host.world.tick} vs ${oldTick}`);
  assert.deepEqual(host.roster().map((r) => r.slot).sort(), [slotA, slotB].sort(), "the old host's player is gone, Bo's is kept");
  assert.equal(host.world.snapshot().boxes.length, boxesBefore);
  // Ann's position survived the move to within what she walked since the snapshot.
  const aNow = eye(host.world, slotA);
  assert.ok(Math.hypot(aNow.x - aBefore.x, aNow.z - aBefore.z) < 1.5);

  // Bo walks; the new host applies it and Bo's own prediction agrees.
  const bStart = eye(host.world, slotB);
  s.run(90, { b: () => ({ buttons: BTN.FORWARD, yaw: 3000, pitch: 0 }) });
  assert.equal(s.b.client?.slot, slotB, 'Bo never lost his slot');
  assert.equal(s.b.client?.rejoining, false, 'Bo was welcomed again');
  const bHost = eye(host.world, slotB);
  assert.ok(Math.hypot(bHost.x - bStart.x, bHost.z - bStart.z) > 3, "the new host applies Bo's inputs");
  s.run(90);
  const bPred = eye(/** @type {PredictedClient} */ (s.b.client).world, slotB);
  const bAuth = eye(host.world, slotB);
  assert.ok(Math.hypot(bPred.x - bAuth.x, bPred.y - bAuth.y, bPred.z - bAuth.z) < 0.01, 'prediction converges on the new host');
  assert.ok(host.world.tick > oldTick, 'the match moved on');
  assert.ok(s.b.client && s.b.client.hostSlot() === slotA, "the HUD's host marker follows the new host");
  assert.deepEqual(s.b.client?.roster.map((r) => r.slot).sort(), [slotA, slotB].sort());
});

test('handoff over a lossy, jittery network still keeps everyone seated', async () => {
  const s = await setup({ latencyMs: 70, jitterMs: 30, drop: { input: 0.2, state: 0.1, reply: 0.2 } });
  s.run(200, { a: (i) => ({ buttons: i % 90 < 45 ? BTN.FORWARD : BTN.BACK, yaw: 0, pitch: 0 }) });
  const slotA = /** @type {number} */ (s.a.client?.slot);
  const slotB = /** @type {number} */ (s.b.client?.slot);
  s.moveHost('b');
  s.run(240, { a: () => ({ buttons: BTN.FORWARD, yaw: 1000, pitch: 0 }) });
  s.run(200); // idle: every input applied, the boxes settle
  const host = /** @type {HostSession} */ (s.b.host);
  assert.equal(host.selfSlot, slotB);
  assert.equal(s.a.client?.slot, slotA);
  assert.equal(s.a.client?.rejoining, false);
  assert.equal(host.peers.size, 1);
  const err = Math.hypot(...(['x', 'y', 'z'].map((k) => /** @type {any} */ (eye(/** @type {PredictedClient} */ (s.a.client).world, slotA))[k] - /** @type {any} */ (eye(host.world, slotA))[k])));
  assert.ok(err < 0.05, `converged: ${err}`);
});

test('a peer the new host never heard of gets a new seat, and its stale inputs are dropped', async () => {
  const s = await setup();
  s.run(120);
  // Bo's hello is lost for good: take his id out of the seat table the successor holds.
  const ann = /** @type {PredictedClient} */ (s.a.client);
  ann.seatTable = { host: 0, seats: (ann.seatTable?.seats ?? []).filter((x) => x.id !== 'b') };
  s.moveHost('a');
  s.run(120, { b: () => ({ buttons: BTN.FORWARD, yaw: 0, pitch: 0 }) });
  const host = /** @type {HostSession} */ (s.a.host);
  assert.ok(host.peers.has('b'), 'Bo said hello and got a seat');
  assert.equal(s.b.client?.rejoining, false);
  assert.equal(s.b.client?.slot, host.peers.get('b')?.slot, 'his page follows the slot the new host gave him');
});

test('no snapshot ever heard: the successor starts a fresh match and the peers join it', async () => {
  const s = await setup();
  // Nothing has run yet, so Ann has no baseline.
  s.moveHost('a');
  assert.ok(s.a.host);
  s.run(200);
  const host = /** @type {HostSession} */ (s.a.host);
  assert.equal(host.peers.size, 1);
  assert.equal(s.b.client?.status, 'playing');
});

test('the old host stepping down becomes a peer of the new one', async () => {
  const s = await setup();
  s.run(90);
  // The platform moved the host while the old page is still alive (a reconnect): both pages get the event.
  s.hub.changeHost('a');
  for (const m of [s.h, s.a, s.b]) {
    const r = handoffRole({ channel: m.channel, world: m.world, name: m.name, host: m.host, client: m.client });
    m.host = r.host;
    m.client = r.client;
  }
  assert.ok(s.a.host && s.h.client && !s.h.host);
  s.run(150);
  assert.equal(s.h.client?.status, 'playing');
  assert.equal(/** @type {HostSession} */ (s.a.host).peers.size, 2);
});

test('over the real Embedded App SDK: the SDK moving the host reaches the game through sdk-channel', async () => {
  const J = await initJolt();
  const { LoopbackHub, SideRoomSDK } = await import('../../../../embedded-app-sdk/src/index.ts');
  const hub = new LoopbackHub('h');
  const stubEnv = () => /** @type {any} */ ({ parent: null, hostOrigin: '', hostWindow: null, addEventListener() {}, removeEventListener() {} });
  /** @param {string} id @param {string} name */
  const member = (id, name) => {
    const sdk = new SideRoomSDK('app', { env: stubEnv(), transport: hub.connect(id) });
    const channel = channelFromSdk(/** @type {any} */ (sdk), { name });
    const world = new World(J, { seed: id === 'h' ? 1234 : 0 });
    return { id, name, sdk, channel, world, host: /** @type {HostSession | null} */ (null), client: /** @type {PredictedClient | null} */ (null) };
  };
  const h = member('h', 'Host');
  const a = member('a', 'Ann');
  const b = member('b', 'Bo');
  h.host = new HostSession({ channel: h.channel, world: h.world, name: h.name });
  a.client = new PredictedClient({ channel: a.channel, world: a.world, name: a.name });
  b.client = new PredictedClient({ channel: b.channel, world: b.world, name: b.name });
  const events = /** @type {string[]} */ ([]);
  for (const m of [a, b]) m.channel.onHostChange?.((host, previous) => events.push(`${m.id}:${previous}>${host}`));
  const play = (/** @type {number} */ n) => {
    for (let i = 0; i < n; i++) {
      for (const m of [h, a, b]) {
        const input = m.id === 'b' ? { buttons: BTN.FORWARD, yaw: 0, pitch: 0 } : idleInput();
        m.host?.tick(input);
        m.client?.tick(input);
      }
    }
  };
  play(120);
  const slotB = /** @type {number} */ (b.client?.slot);
  assert.ok(slotB > 0);
  assert.equal(a.channel.hostId, 'h');

  // The old host's page closes; the platform names Ann and each SDK hears HOST_CHANGED (what ActivityData.changeHost does).
  h.host?.close();
  a.sdk.data.changeHost('a');
  b.sdk.data.changeHost('a');
  assert.deepEqual(events.sort(), ['a:h>a', 'b:h>a']);
  assert.equal(a.channel.isHost, true);
  assert.equal(b.channel.hostId, 'a');
  for (const m of [a, b]) {
    const r = handoffRole({ channel: m.channel, world: m.world, name: m.name, host: m.host, client: m.client });
    m.host = r.host;
    m.client = r.client;
  }
  assert.ok(a.host && b.client);
  play(150);
  assert.equal(b.client?.slot, slotB);
  assert.equal(a.host?.peers.get('b')?.slot, slotB);
  assert.ok((a.host?.peers.get('b')?.ack ?? 0) > 0, "Bo's inputs reach the new host");
  assert.ok(b.client && b.client.stats.snapshots > 0 && b.client.lastSnapTick > 0, 'Bo hears the new host snapshots');
  // A peer still cannot write the host keys.
  assert.throws(() => b.sdk.state.set({ 'host:snap': 'x' }), /Only the host/);
});
