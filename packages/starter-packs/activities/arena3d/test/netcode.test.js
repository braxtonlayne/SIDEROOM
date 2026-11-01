// @ts-check
// Host authority, client-side prediction and reconciliation, and interpolation, over the in-process loopback
// with simulated latency, jitter and loss (seeded, virtual time: reproducible).
import assert from 'node:assert/strict';
import initJolt from 'jolt-physics/wasm-compat';
import { HostSession } from '../net/host.js';
import { PredictedClient } from '../net/client.js';
import { createLoopbackHub } from '../net/loopback.js';
import { World } from '../sim/world.js';
import { BTN, idleInput } from '../sim/input.js';

const TICK_MS = 1000 / 60;

/**
 * @param {{ latencyMs: number, jitterMs: number, drop: { input?: number, state?: number, reply?: number }, peers: number }} net
 */
async function setup(net) {
  const J = await initJolt();
  const hub = createLoopbackHub({ hostId: 'h', latencyMs: net.latencyMs, jitterMs: net.jitterMs, drop: net.drop, seed: 99 });
  const host = new HostSession({ channel: hub.join('h', 'Host'), world: new World(J, { seed: 1234 }), name: 'Host' });
  const clients = [];
  for (let i = 0; i < net.peers; i++) {
    // A peer's world can start from any seed: the first snapshot overwrites everything.
    clients.push(new PredictedClient({ channel: hub.join(`p${i}`, `Peer ${i}`), world: new World(J, { seed: 0 }), name: `Peer ${i}` }));
  }
  let t = 0;
  /** @param {(tick: number) => import('../sim/input.js').PlayerInput[]} inputs per peer */
  const run = (ticks, inputs) => {
    for (let i = 0; i < ticks; i++) {
      t += TICK_MS;
      hub.pump(t);
      host.tick(idleInput());
      const ins = inputs(i);
      clients.forEach((c, k) => {
        c.tick(ins[k] ?? idleInput());
        c.frame();
      });
    }
  };
  return { J, hub, host, clients, run };
}

/** @param {World} world @param {number} slot */
const pos = (world, slot) => {
  const e = world.eye(slot);
  assert.ok(e, `slot ${slot} exists`);
  return e;
};

/** A walk-turn-jump-crouch routine, the same for any start tick. @param {number} i */
function routine(i) {
  if (i < 60) return { buttons: BTN.FORWARD, yaw: 0, pitch: 0 };
  if (i < 120) return { buttons: BTN.FORWARD | (i === 80 ? BTN.JUMP : 0), yaw: 8000, pitch: 0 };
  if (i < 160) return { buttons: BTN.LEFT | BTN.CROUCH, yaw: 8000, pitch: 0 };
  if (i < 220) return { buttons: BTN.FORWARD | BTN.RIGHT | (i % 40 === 0 ? BTN.JUMP : 0), yaw: -12000, pitch: 0 };
  return idleInput();
}

test('peers join over the lossy network and get distinct slots', async () => {
  const s = await setup({ latencyMs: 40, jitterMs: 10, drop: { input: 0.3, reply: 0.3 }, peers: 3 });
  s.run(120, () => []);
  const slots = s.clients.map((c) => c.slot);
  assert.ok(slots.every((x) => x > 0), `all welcomed: ${slots}`);
  assert.equal(new Set(slots).size, 3);
  assert.deepEqual(s.host.roster().map((r) => r.slot), [0, ...slots].sort((a, b) => a - b));
  assert.ok(s.clients.every((c) => c.roster.length === 4), 'everyone heard the roster');
});

test('prediction and reconciliation converge under latency, jitter and loss', async () => {
  const s = await setup({ latencyMs: 60, jitterMs: 30, drop: { input: 0.15, state: 0.1, reply: 0.2 }, peers: 1 });
  const c = s.clients[0];
  s.run(60, () => []); // join
  assert.equal(c.status, 'playing');
  const start = pos(s.host.world, c.slot);
  let worst = 0;
  let i0 = 0;
  s.run(220, (i) => {
    i0 = i;
    return [routine(i)];
  });
  void i0;
  worst = c.stats.maxCorrection;
  // Idle long enough for every input to be applied and the boxes to settle.
  s.run(150, () => [idleInput()]);
  const h = pos(s.host.world, c.slot);
  const p = pos(c.world, c.slot);
  const moved = Math.hypot(h.x - start.x, h.z - start.z);
  assert.ok(moved > 5, `the player really moved (${moved.toFixed(2)} m)`);
  const err = Math.hypot(h.x - p.x, h.y - p.y, h.z - p.z);
  assert.ok(err < 0.01, `predicted and authoritative positions converge: ${err.toFixed(5)} m apart`);
  assert.ok(c.pending.length < 30, `only about one round trip of inputs is pending: ${c.pending.length}`);
  assert.ok(c.stats.resimTicks > 100, 'it rewound and re-simulated');
  assert.ok(worst < 1, `no correction larger than 1 m while moving (worst ${worst.toFixed(3)})`);
  // The visual offset has decayed.
  assert.ok(Math.hypot(...c.offset) < 0.01);
  console.log(`convergence: moved ${moved.toFixed(2)} m, final error ${err.toExponential(2)} m, corrections ${c.stats.corrections}, worst ${worst.toFixed(3)} m, dropped ${s.hub.stats.dropped}/${s.hub.stats.sent}`);
});

test('with a perfect network the prediction needs no visible correction', async () => {
  const s = await setup({ latencyMs: 50, jitterMs: 0, drop: {}, peers: 1 });
  const c = s.clients[0];
  s.run(60, () => []);
  s.run(220, (i) => [routine(i)]);
  assert.ok(c.stats.maxCorrection < 0.05, `worst correction ${c.stats.maxCorrection.toFixed(4)} m`);
});

test('a desync is corrected by the next snapshot', async () => {
  const s = await setup({ latencyMs: 30, jitterMs: 0, drop: {}, peers: 1 });
  const c = s.clients[0];
  s.run(60, () => []);
  // Teleport the peer's predicted player 1.5 m sideways: the host never saw that happen.
  const e = pos(c.world, c.slot);
  const p = /** @type {any} */ (c.world.players[c.slot]);
  c.world.r3.Set(e.x + 1.5, e.y - 1.6, e.z);
  p.character.SetPosition(c.world.r3);
  s.run(20, () => [idleInput()]);
  const h = pos(s.host.world, c.slot);
  const q = pos(c.world, c.slot);
  assert.ok(Math.hypot(h.x - q.x, h.z - q.z) < 0.01);
  assert.ok(c.stats.maxCorrection > 1.4);
});

test('remote players are interpolated close to where the host has them', async () => {
  const s = await setup({ latencyMs: 40, jitterMs: 20, drop: { state: 0.1 }, peers: 2 });
  const [a, b] = s.clients;
  s.run(60, () => []);
  s.run(220, (i) => [routine(i), idleInput()]);
  s.run(120, () => []);
  const seen = b.remote(a.slot);
  assert.ok(seen, 'b sees a');
  const truth = pos(s.host.world, a.slot);
  const err = Math.hypot(seen.p[0] - truth.x, seen.p[2] - truth.z);
  assert.ok(err < 0.02, `b draws a ${err.toFixed(4)} m from the host's position once a stops`);
});

test('the host drops a silent peer and a peer that says bye', async () => {
  const s = await setup({ latencyMs: 20, jitterMs: 0, drop: {}, peers: 2 });
  s.host.timeoutTicks = 120;
  s.run(60, () => []);
  const [a, b] = s.clients;
  a.leave();
  s.run(10, () => []);
  assert.equal(s.host.world.players[a.slot], null, 'bye frees the slot');
  // b goes silent (stops ticking): closing its listeners and not sending anything.
  b.offs.forEach((off) => off());
  s.clients.splice(0);
  s.run(150, () => []);
  assert.equal(s.host.world.players[b.slot], null, 'silence frees the slot');
  assert.deepEqual(s.host.roster().map((r) => r.slot), [0]);
});

test('a ninth player is told the arena is full', async () => {
  const s = await setup({ latencyMs: 0, jitterMs: 0, drop: {}, peers: 8 });
  s.run(10, () => []);
  assert.equal(s.clients.filter((c) => c.status === 'playing').length, 7);
  assert.equal(s.clients.filter((c) => c.status === 'full').length, 1);
});
