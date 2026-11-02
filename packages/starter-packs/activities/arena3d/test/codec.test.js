// @ts-check
// The wire codec: every message round-trips, snapshots are lossless for float32 state and compact, and the
// decoder refuses anything malformed instead of throwing.
import assert from 'node:assert/strict';
import initJolt from 'jolt-physics/wasm-compat';
import {
  MSG, PROTOCOL_VERSION, MAX_INPUT_FRAMES, decode, encodeBye, encodeFull, encodeHello, encodeInput, encodeRoster,
  encodeSnapshot, encodeWelcome,
} from '../net/codec.js';
import { fromBase64, toBase64 } from '../net/channel.js';
import { World } from '../sim/world.js';

test('hello, welcome, full, bye and roster round-trip', () => {
  assert.deepEqual(decode(encodeHello('Ada')), { type: MSG.HELLO, name: 'Ada' });
  assert.deepEqual(decode(encodeWelcome(3, 0xdeadbeef, 12345)), { type: MSG.WELCOME, slot: 3, seed: 0xdeadbeef, tick: 12345 });
  assert.deepEqual(decode(encodeFull()), { type: MSG.FULL });
  assert.deepEqual(decode(encodeBye()), { type: MSG.BYE });
  const roster = [{ slot: 0, name: 'Host', bot: false }, { slot: 2, name: 'Tofu bot', bot: true }, { slot: 5, name: 'Zoë ✓', bot: false }];
  assert.deepEqual(decode(encodeRoster(roster)), { type: MSG.ROSTER, roster });
});

test('names are cut to 24 bytes on the wire', () => {
  const m = decode(encodeHello('x'.repeat(100)));
  assert.ok(m && m.type === MSG.HELLO);
  assert.equal(m.name.length, 24);
});

test('input frames round-trip with their seqs, newest 32 only', () => {
  const frames = Array.from({ length: 40 }, (_, i) => ({ seq: 1000 + i, input: { buttons: i % 64, yaw: -32768 + i * 1000, pitch: i * 100 - 2000 } }));
  const m = decode(encodeInput(frames));
  assert.ok(m && m.type === MSG.INPUT);
  assert.equal(m.frames.length, MAX_INPUT_FRAMES);
  assert.deepEqual(m.frames, frames.slice(-MAX_INPUT_FRAMES));
  assert.equal(encodeInput(frames).length, 2 + 4 + 1 + MAX_INPUT_FRAMES * 5);
});

test('a real snapshot round-trips exactly and is small', async () => {
  const J = await initJolt();
  const w = new World(J, { seed: 42 });
  w.addPlayer(0);
  w.addPlayer(3);
  w.setInput(0, { buttons: 1 | 16, yaw: 1234, pitch: -500 });
  w.setInput(3, { buttons: 32 | 4, yaw: -8000, pitch: 0 });
  for (let i = 0; i < 75; i++) w.step();
  const snap = w.snapshot();
  const acks = new Map([[0, 0], [3, 987654]]);
  const bytes = encodeSnapshot(snap, acks);
  const m = decode(bytes);
  assert.ok(m && m.type === MSG.SNAPSHOT);
  assert.deepEqual(m.snap, snap, 'float32 values survive float32 encoding');
  assert.equal(m.acks.get(3), 987654);
  assert.ok(bytes.length < 1024, `snapshot is ${bytes.length} bytes`);
  // Applying the decoded snapshot to another world reproduces the hashed state.
  const w2 = new World(J, { seed: 1 });
  w2.applySnapshot(m.snap);
  assert.equal(w2.stateHash(), w.stateHash());
  w.destroy();
  w2.destroy();
});

test('the decoder refuses other versions, unknown types, truncation, trailing bytes and NaN', () => {
  const ok = encodeWelcome(1, 2, 3);
  assert.equal(ok[0], PROTOCOL_VERSION);
  const otherVersion = ok.slice();
  otherVersion[0] = PROTOCOL_VERSION + 1;
  assert.equal(decode(otherVersion), null);
  assert.equal(decode(new Uint8Array([PROTOCOL_VERSION, 99])), null);
  assert.equal(decode(ok.subarray(0, ok.length - 1)), null);
  assert.equal(decode(new Uint8Array([...ok, 0])), null);
  assert.equal(decode(new Uint8Array([])), null);
  assert.equal(decode(null), null);
  assert.equal(decode(encodeWelcome(200, 0, 0)), null, 'slot out of range');
  // A snapshot with a NaN position.
  const snap = { tick: 1, rng: 1, score: 0, players: [{ slot: 0, crouched: false, prevButtons: 0, yaw: 0, pitch: 0, dvx: 0, dvz: 0, p: /** @type {[number, number, number]} */ ([NaN, 0, 0]), v: /** @type {[number, number, number]} */ ([0, 0, 0]) }], boxes: [] };
  assert.equal(decode(encodeSnapshot(snap, new Map())), null);
  // Random garbage never throws.
  let seed = 7;
  for (let i = 0; i < 2000; i++) {
    const n = (seed = (seed * 1103515245 + 12345) >>> 0) % 64;
    const b = new Uint8Array(n);
    for (let j = 0; j < n; j++) b[j] = (seed = (seed * 1103515245 + 12345) >>> 0) >>> 24;
    if (n > 0) b[0] = PROTOCOL_VERSION;
    decode(b);
  }
});

test('base64 helpers round-trip binary', () => {
  const b = new Uint8Array(70000).map((_, i) => (i * 31) & 255);
  assert.deepEqual(fromBase64(toBase64(b)), b);
  assert.equal(fromBase64('***'), null);
});
