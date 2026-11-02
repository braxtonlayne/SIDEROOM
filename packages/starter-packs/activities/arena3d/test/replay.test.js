// @ts-check
// THE GATE (ACT-A3D-001): the recorded input script (900 ticks, 2 players, a pyramid of boxes and boxes dropped
// from the sky) run in two separate Jolt module instances gives the identical state hash at every 60th tick
// and at the end, both the quantised hash and the float32-exact one.
import assert from 'node:assert/strict';
import initJolt from 'jolt-physics/wasm-compat';
import { runReplay } from '../replay/run.js';
import script from '../replay/script.js';
import { World } from '../sim/world.js';

test('the script is long enough and has two players', () => {
  assert.ok(script.ticks >= 600);
  assert.equal(script.players, 2);
  assert.ok(script.events.some((e) => e[1] === 0) && script.events.some((e) => e[1] === 1));
});

test('two separate Jolt instances agree on the state hash at every 60th tick and at the end', async () => {
  const J1 = await initJolt();
  const J2 = await initJolt();
  assert.notEqual(J1, J2, 'two module instances, two WASM heaps');
  const a = runReplay(J1, script);
  const b = runReplay(J2, script);
  assert.equal(a.checkpoints.length, Math.floor(script.ticks / 60));
  for (let i = 0; i < a.checkpoints.length; i++) {
    assert.equal(b.checkpoints[i].tick, a.checkpoints[i].tick);
    assert.equal(b.checkpoints[i].hash, a.checkpoints[i].hash, `quantised hash differs at tick ${a.checkpoints[i].tick}`);
    assert.equal(b.checkpoints[i].exact, a.checkpoints[i].exact, `exact hash differs at tick ${a.checkpoints[i].tick}`);
  }
  assert.equal(b.final, a.final);
  assert.equal(b.finalExact, a.finalExact);
  // The run is not trivial: the state changes between checkpoints, and the players moved boxes around.
  assert.equal(new Set(a.checkpoints.map((c) => c.hash)).size, a.checkpoints.length, 'every checkpoint is a new state');
  const pyramid = a.boxMoved.slice(0, 6);
  assert.ok(pyramid.some((d) => d > 0.5), `the players knocked the pyramid about: ${pyramid.map((d) => d.toFixed(2)).join(', ')}`);
  console.log(`replay final hash ${a.final} (exact ${a.finalExact}) after ${a.ticks} ticks`);
});

test('the same module, run twice from scratch, also agrees (no hidden state survives a World)', async () => {
  const J = await initJolt();
  const a = runReplay(J, script);
  const b = runReplay(J, script);
  assert.equal(b.finalExact, a.finalExact);
});

test('a different seed or one changed input gives a different hash', async () => {
  const J = await initJolt();
  const base = runReplay(J, { ...script, ticks: 240 });
  const seeded = runReplay(J, { ...script, ticks: 240, seed: script.seed + 1 });
  assert.notEqual(seeded.final, base.final);
  const events = script.events.map((e) => /** @type {[number, number, number, number, number]} */ ([...e]));
  const i = events.findIndex((e) => e[0] >= 30 && e[1] === 0);
  events[i][3] += 1; // one yaw unit (1/65536 of a turn) on one input
  const nudged = runReplay(J, { ...script, ticks: 240, events });
  assert.notEqual(nudged.finalExact, base.finalExact);
});

test('a snapshot restores the hashed state exactly when applied to a fresh world at the same tick', async () => {
  const J = await initJolt();
  const a = new World(J, { seed: 7 });
  a.addPlayer(0);
  a.addPlayer(1);
  a.setInput(0, { buttons: 1, yaw: 0, pitch: 0 });
  for (let i = 0; i < 90; i++) a.step();
  const snap = a.snapshot();
  const b = new World(J, { seed: 7 });
  b.applySnapshot(snap);
  assert.equal(b.tick, a.tick);
  assert.equal(b.stateHash(), a.stateHash());
  a.destroy();
  b.destroy();
});
