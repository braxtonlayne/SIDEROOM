// @ts-check
// sim/: the PRNG, the hasher, input sanitising, and the game rules on top of Jolt (jump on press, crouch that
// cannot stand up under a ceiling, boxes that respawn from the PRNG, the pad score), plus the input log being a
// faithful replay script.
import assert from 'node:assert/strict';
import initJolt from 'jolt-physics/wasm-compat';
import { Rng, seedFromString } from '../sim/rng.js';
import { StateHasher } from '../sim/hash.js';
import { BTN, sanitiseInput, wrapYaw, PITCH_LIMIT } from '../sim/input.js';
import { InputLog } from '../sim/input-log.js';
import { ARENA } from '../sim/level.js';
import { World } from '../sim/world.js';
import { runReplay } from '../replay/run.js';

/** @type {Awaited<ReturnType<typeof initJolt>>} */
let J;
beforeAll(async () => {
  J = await initJolt();
});

test('the PRNG is a pure function of its seed, with known first values', () => {
  const a = new Rng(seedFromString('room-1'));
  const b = new Rng(seedFromString('room-1'));
  const xs = Array.from({ length: 5 }, () => a.nextU32());
  assert.deepEqual(Array.from({ length: 5 }, () => b.nextU32()), xs);
  // Pinned, so a change to the generator is a deliberate one (it would change every recorded match).
  assert.deepEqual(Array.from({ length: 3 }, (() => { const r = new Rng(1); return () => r.nextU32(); })()), [2693262067, 11749833, 2265367787]);
  for (let i = 0; i < 1000; i++) {
    const v = a.range(-3, 5);
    assert.ok(v >= -3 && v < 5 && Number.isInteger(v * 1024));
  }
});

test('the hasher is stable and sensitive', () => {
  const h = new StateHasher().u32(1).u32(2).q(1.23456, 1e4).f32(0.1).hex();
  assert.equal(h, new StateHasher().u32(1).u32(2).q(1.23456, 1e4).f32(0.1).hex());
  assert.notEqual(h, new StateHasher().u32(1).u32(2).q(1.23466, 1e4).f32(0.1).hex());
  assert.match(h, /^[0-9a-f]{16}$/);
  assert.notEqual(h.slice(8), '00000000');
});

test('inputs are cleaned to their integer ranges', () => {
  assert.deepEqual(sanitiseInput({ buttons: 255, yaw: 40000, pitch: 99999 }), { buttons: 63, yaw: wrapYaw(40000), pitch: PITCH_LIMIT });
  assert.equal(wrapYaw(40000), 40000 - 65536);
  assert.deepEqual(sanitiseInput(null), { buttons: 0, yaw: 0, pitch: 0 });
  assert.deepEqual(sanitiseInput(/** @type {any} */ ({ buttons: 'x', yaw: NaN, pitch: 1.7 })), { buttons: 0, yaw: 0, pitch: 2 });
});

test('jump fires on the press, not while held', () => {
  const w = new World(J, { seed: 1 });
  w.addPlayer(0);
  for (let i = 0; i < 30; i++) w.step(); // settle on the floor
  const y0 = w.eye(0)?.y ?? 0;
  w.setInput(0, { buttons: BTN.JUMP, yaw: 0, pitch: 0 });
  let peak = 0;
  for (let i = 0; i < 90; i++) {
    w.step();
    peak = Math.max(peak, (w.eye(0)?.y ?? 0) - y0);
  }
  assert.ok(peak > 1, `jumped ${peak.toFixed(2)} m`);
  // Still holding jump: back on the ground and staying there.
  const ys = [];
  for (let i = 0; i < 60; i++) {
    w.step();
    ys.push((w.eye(0)?.y ?? 0) - y0);
  }
  assert.ok(Math.max(...ys) < 0.05, 'no second jump while the button is held');
  w.destroy();
});

test('a crouched player cannot stand up under a low ceiling', () => {
  // A slab whose underside is 1.4 m up: room for a crouched player (1.1 m), not a standing one (1.8 m).
  const level = { ...ARENA, randomBoxes: 0, boxes: [], statics: [...ARENA.statics, { pos: /** @type {[number, number, number]} */ ([0, 1.65, 4]), half: /** @type {[number, number, number]} */ ([2, 0.25, 1.5]), color: /** @type {[number, number, number]} */ ([1, 1, 1]) }] };
  const w = new World(J, { seed: 1, level });
  w.addPlayer(0); // spawn (0, 0, 8), looking down -Z
  for (let i = 0; i < 20; i++) w.step();
  w.setInput(0, { buttons: BTN.CROUCH, yaw: 0, pitch: 0 });
  for (let i = 0; i < 5; i++) w.step();
  assert.equal(w.players[0]?.crouched, true);
  w.setInput(0, { buttons: BTN.CROUCH | BTN.FORWARD, yaw: 0, pitch: 0 });
  for (let i = 0; i < 180 && (w.eye(0)?.z ?? 9) > 4.2; i++) w.step();
  assert.ok(Math.abs((w.eye(0)?.z ?? 0) - 4) < 0.6, `under the slab at z=${w.eye(0)?.z}`);
  w.setInput(0, { buttons: 0, yaw: 0, pitch: 0 });
  for (let i = 0; i < 30; i++) w.step();
  assert.equal(w.players[0]?.crouched, true, 'still crouched: standing would hit the slab');
  w.destroy();
});

test('a box that falls out respawns at a PRNG spot, and boxes on the pad score', () => {
  const w = new World(J, { seed: 3 });
  const box = w.boxes[0];
  const rng0 = w.rng.state;
  w.r3.Set(0, -20, 0);
  w.bi.SetPosition(box.id, w.r3, J.EActivation_Activate);
  w.step();
  const p = w.bi.GetPosition(box.id);
  assert.ok(p.GetY() > 5, 'dropped back in from above');
  assert.notEqual(w.rng.state, rng0, 'the PRNG chose the spot');
  assert.equal(w.score, 0);
  const pad = w.level.pad;
  w.r3.Set((pad.min[0] + pad.max[0]) / 2, 0.6, (pad.min[1] + pad.max[1]) / 2);
  w.bi.SetPosition(w.boxes[1].id, w.r3, J.EActivation_Activate);
  for (let i = 0; i < 30; i++) w.step();
  assert.equal(w.score, 1);
  w.destroy();
});

test('the input log is a replay script: replaying it reproduces the state hash', () => {
  const w = new World(J, { seed: 11 });
  w.addPlayer(0);
  w.addPlayer(1);
  const log = new InputLog();
  const rng = new Rng(77);
  for (let t = 0; t < 300; t++) {
    for (const slot of [0, 1]) {
      if (t % 20 === 0) {
        const input = sanitiseInput({ buttons: rng.int(64) & ~BTN.CROUCH, yaw: rng.int(65536) - 32768, pitch: 0 });
        w.setInput(slot, input);
        log.record(w.tick, slot, input);
      }
    }
    w.step();
  }
  const hash = w.stateHash({ exact: true });
  w.destroy();
  const r = runReplay(J, { version: 1, seed: 11, ticks: 300, players: 2, events: log.snapshot() });
  assert.equal(r.finalExact, hash);
});
