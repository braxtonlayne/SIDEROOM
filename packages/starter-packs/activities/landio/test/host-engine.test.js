// @ts-check
// The host engine on its own (no channel): what it broadcasts, how much, and how it treats a member.
import assert from 'node:assert/strict';
import { LandioHost, roomLabel, cleanName } from '../host/engine.js';

const TICK_MS = 1000 / 7;

/** An engine with a fake clock whose outbound traffic is recorded. */
function engine(opt = {}) {
  const clock = { t: 5000 };
  const sent = /** @type {{to: string, msg: any}[]} */ ([]);
  const frames = /** @type {any[]} */ ([]);
  const eng = new LandioHost({
    instance: 'inst-xyz',
    hostId: 'h',
    now: () => clock.t,
    send: (to, msg) => sent.push({ to, msg }),
    publish: (f) => frames.push(f),
    seed: 'engine-seed',
    ...opt,
  });
  const tick = (n = 1) => {
    for (let i = 0; i < n; i++) {
      clock.t += TICK_MS;
      eng.tick();
    }
  };
  return { eng, clock, sent, frames, tick };
}

test('an empty room is paused: no ticks, no frames', () => {
  const e = engine();
  e.tick(10);
  assert.equal(e.frames.length, 0);
  assert.equal(e.eng.state.tick, 0);
});

test('hello seats a member with the SDK name, the first welcome is a full snapshot, the host hosts', () => {
  const e = engine();
  e.eng.handle('h', 'Alice', { t: 'hello', name: 'ignored' });
  e.eng.handle('g', 'Bob\u0000<b>', { t: 'hello' });
  const w = e.sent.filter((s) => s.msg.t === 'welcome');
  assert.equal(w.length, 2);
  assert.ok(w[0].msg.full && w[0].msg.cfg && w[0].msg.room.code);
  assert.equal(w[0].msg.you.host, true);
  assert.equal(w[1].msg.you.host, false);
  assert.deepEqual(e.eng.arena.gameInfo().people, ['Alice', 'Bobb'], 'control characters and angle brackets are stripped');
  assert.equal(cleanName('  x<y>\u0007 '), 'xy');
});

test('frames carry one record, versions, and the shared parts only when they changed', () => {
  const e = engine();
  e.eng.handle('h', 'Alice', { t: 'hello' });
  e.tick(6);
  assert.equal(e.frames.length, 6);
  const f = e.frames;
  assert.ok(f.every((x) => x.recs.length === 1 && x.recs[0].t === x.now));
  assert.ok(f[0].roster && f[0].round && f[0].game, 'the first frame carries the shared parts');
  assert.ok(f.slice(2).every((x) => !x.roster && !x.round && !x.game), 'later frames do not repeat them');
  assert.ok(f.every((x) => typeof x.rv === 'number' && typeof x.rdv === 'number' && typeof x.gv === 'number'));
  assert.ok(f.filter((x) => x.stats).length >= 2 && f.filter((x) => !x.stats).length >= 2, 'stats every other tick');
});

test('redundancy repeats the newest records in each frame', () => {
  const e = engine({ redundancy: 3 });
  e.eng.handle('h', 'Alice', { t: 'hello' });
  e.tick(5);
  assert.deepEqual(e.frames[4].recs.map((r) => r.t), [3, 4, 5]);
});

test('"you" goes to a member only when it changes, not every tick', () => {
  const e = engine();
  e.eng.handle('h', 'Alice', { t: 'hello' });
  e.eng.handle('h', 'Alice', { t: 'play', id: 1 });
  e.sent.length = 0;
  e.tick(40);
  const yous = e.sent.filter((s) => s.msg.t === 'you');
  assert.ok(yous.length <= 4, `${yous.length} "you" messages in 40 ticks`);
});

test('inputs are applied once, in order, and acknowledged in the frame', () => {
  const e = engine();
  e.eng.handle('h', 'Alice', { t: 'hello' });
  e.eng.handle('h', 'Alice', { t: 'play', id: 7 });
  const pid = e.sent.find((s) => s.msg.t === 'res' && s.msg.id === 7)?.msg.res.pid;
  assert.ok(pid);
  e.eng.handle('h', 'Alice', { t: 'in', acts: [[1, 1, null]] });
  e.eng.handle('h', 'Alice', { t: 'in', acts: [[1, 1, null], [2, 2, null]] }); // seq 1 again: ignored
  e.tick(3);
  const last = e.frames.at(-1);
  const mine = last.y.find((/** @type {any[]} */ r) => r[0] === 'h');
  assert.equal(mine[1], 2);
  assert.equal(e.eng.state.players.find((p) => p.id === pid).dir, 2);
});

test('only the host changes settings and removes people; a guest\'s attempt is refused', () => {
  const e = engine();
  e.eng.handle('h', 'Alice', { t: 'hello' });
  e.eng.handle('g', 'Bob', { t: 'hello' });
  e.sent.length = 0;
  const bots = e.eng.state.cfg.targetHeads;
  e.eng.handle('g', 'Bob', { t: 'settings', id: 1, body: { bots: 9 } });
  e.eng.handle('g', 'Bob', { t: 'host', id: 2, body: { action: 'end' } });
  const res = e.sent.filter((s) => s.msg.t === 'res').map((s) => s.msg.res.error);
  assert.deepEqual(res, ['not-host', 'not-host']);
  assert.equal(e.eng.state.cfg.targetHeads, bots);
});

test('malformed and unknown messages are ignored', () => {
  const e = engine();
  e.eng.handle('h', 'Alice', { t: 'hello' });
  for (const m of [null, 5, 'x', {}, { t: 7 }, { t: 'nope' }, { t: 'in', acts: 'no' }, { t: 'in', acts: [[1, 99]] }, { t: 'settings', id: 1, body: 5 }])
    e.eng.handle('h', 'Alice', /** @type {any} */ (m));
  e.eng.handle('bad id with spaces', 'X', { t: 'hello' });
  assert.equal(e.eng.arena.seats.size, 1);
  e.tick(3);
});

test('roomLabel is short and safe', () => {
  assert.equal(roomLabel('abc-DEF-123456'), 'ABC-DEF-123456'.replace(/-/g, '').slice(-6));
  assert.equal(roomLabel('!!'), 'ROOM');
});

test('traffic: 12 people with bots on a 100x100 arena stay inside the datachannel budget (256 kbps = 32 KB/s per member)', () => {
  const e = engine({ cfg: { targetHeads: 20 } });
  for (let i = 0; i < 12; i++) {
    const id = i === 0 ? 'h' : `m${i}`;
    e.eng.handle(id, `P${i}`, { t: 'hello' });
    e.eng.handle(id, `P${i}`, { t: 'play', id: 1 });
  }
  const seconds = 40;
  e.tick(seconds * 7);
  const frameBytes = e.eng.bytes.frames / seconds;
  const addressed = e.eng.bytes.addressed / seconds;
  const biggest = Math.max(...e.frames.map((f) => JSON.stringify(f).length));
  console.log(`12 people, 100x100, 20 bots: broadcast ${(frameBytes / 1024).toFixed(1)} KB/s, addressed ${(addressed / 1024).toFixed(2)} KB/s, biggest frame ${biggest} B`);
  // The host sends the frame once; the SFU fans it out. The host's own uplink is frames + addressed.
  assert.ok((frameBytes + addressed) / 1024 < 32, `${((frameBytes + addressed) / 1024).toFixed(1)} KB/s`);
});

test('traffic: 24 people with bots on a 200x200 arena (the largest) fit the budget too', () => {
  const e = engine({ cfg: { w: 200, h: 200, targetHeads: 30 } });
  for (let i = 0; i < 24; i++) {
    const id = i === 0 ? 'h' : `m${i}`;
    e.eng.handle(id, `P${i}`, { t: 'hello' });
    e.eng.handle(id, `P${i}`, { t: 'play', id: 1 });
  }
  const seconds = 30;
  e.tick(seconds * 7);
  const kb = (e.eng.bytes.frames + e.eng.bytes.addressed) / seconds / 1024;
  const biggest = Math.max(...e.frames.map((f) => JSON.stringify(f).length));
  console.log(`24 people, 200x200, 30 bots: host uplink ${kb.toFixed(1)} KB/s, biggest frame ${biggest} B`);
  assert.ok(kb < 32, `${kb.toFixed(1)} KB/s`);
});
