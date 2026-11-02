// @ts-check
// Host migration, the game half (D-388, ACT-LND-002): the successor rule, a compact snapshot a successor resumes
// from identically, and the HostHandoff flow over the loopback channel (the hub moves the host role as the
// platform will once it can).
import assert from 'node:assert/strict';
import { LandioHost } from '../host/engine.js';
import { SNAPSHOT_VERSION, restoreState, resumeEngine, snapshotEngine, snapshotState, successor, successorFromSnapshot } from '../host/handoff.js';
import { Reassembler, chunkMessage, decodeMessage, encodeMessage } from '../client/net/channel.js';
import { createLoopbackHub } from '../client/net/loopback.js';
import { createSdkTransport } from '../client/net/sdk-transport.js';
import { World } from '../client/world.js';
import { createState, step, stateHash } from '../shared/rules.js';

const TICK_MS = 1000 / 7;
const flush = async () => {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
};

/** Send a value over the wire the way the channel does: JSON bytes, cut into small pieces, put back together. @param {any} v */
function overTheWire(v) {
  const r = new Reassembler();
  let out;
  for (const piece of chunkMessage(v, 3000)) out = r.push('st:snap', decodeMessage(encodeMessage(piece)));
  return out;
}

test('successor: the longest-connected member, ties broken by member id, never the one leaving', () => {
  assert.equal(successor([{ id: 'b', since: 5 }, { id: 'a', since: 9 }, { id: 'c', since: 2 }], 'c'), 'b');
  assert.equal(successor([{ id: 'zed', since: 1 }, { id: 'amy', since: 1 }, { id: 'bob', since: 1 }]), 'amy', 'a tie: smaller id');
  // Plain code-unit order, the same in every browser: upper case sorts before lower case.
  assert.equal(successor([{ id: 'b', since: 1 }, { id: 'B', since: 1 }]), 'B');
  assert.equal(successor([{ id: 'h', since: 0 }], 'h'), null);
  assert.equal(successor([]), null);
});

test('successorFromSnapshot: seated members by when they sat down, then the rest by id; everyone agrees', () => {
  const snap = /** @type {any} */ ({ host: 'h1', seats: [{ uid: 'h1', order: 0 }, { uid: 'g9', order: 1 }, { uid: 'g2', order: 2 }] });
  assert.equal(successorFromSnapshot(snap, ['h1', 'g2', 'g9', 'aaa']), 'g9');
  assert.equal(successorFromSnapshot(snap, ['g2', 'aaa', 'h1']), 'g2');
  assert.equal(successorFromSnapshot(snap, ['aaa', 'abc']), 'aaa', 'nobody seated: smallest id');
  // Every member computes it from the same snapshot, whatever order its member list is in.
  assert.equal(successorFromSnapshot(snap, ['aaa', 'g9', 'g2']), successorFromSnapshot(snap, ['g2', 'g9', 'aaa']));
});

test('a state snapshot is JSON-safe, compact and restores to the same hash', () => {
  const s = createState({ w: 100, h: 100, targetHeads: 12, mode: 'draft' }, 'snap', { fill: true });
  step(s, [], 400);
  const json = JSON.stringify(snapshotState(s));
  const back = restoreState(JSON.parse(json));
  assert.equal(stateHash(back), stateHash(s));
  assert.ok(json.length < 40_000, `snapshot ${json.length} chars`);
  for (let i = 0; i < 300; i++) {
    step(s);
    step(back);
  }
  assert.equal(stateHash(back), stateHash(s), 'the copy plays on identically');
});

/** An engine on a fake clock with three seated members who play. */
function threeSeats(mode = 'draft') {
  const clock = { t: 1000 };
  const e = new LandioHost({ instance: 'inst-1', hostId: 'h1', send() {}, publish() {}, now: () => clock.t, seed: 'handoff', cfg: { targetHeads: 6, mode, w: 60, h: 60 } });
  for (const [id, name] of [['h1', 'Alice'], ['g1', 'Bob'], ['g2', 'Cy']]) {
    e.handle(id, name, { t: 'hello' });
    e.handle(id, name, { t: 'play', id: 1 });
  }
  return { e, clock };
}

/**
 * Members' inputs for tick k: turns that keep heads busy, and a card pick now and then. Sequence numbers rise by
 * 3 a tick so each member has room for a turn and a pick. @param {number} k @param {number} seq
 */
const script = (k, seq) => [
  ['h1', 'Alice', { t: 'in', acts: [[seq * 3, [1, 2, 3, 0][k % 4]]] }],
  ['g1', 'Bob', { t: 'in', acts: [[seq * 3, [2, 3, 0, 1][(k >> 1) % 4]], ...(k % 9 === 0 ? [[seq * 3 + 1, -5]] : [])] }],
  ['g2', 'Cy', { t: 'in', acts: [[seq * 3, [3, 0, 1, 2][(k >> 2) % 4], 0]] }],
];

test('a handoff between two simulated hosts resumes identically', () => {
  const { e: oldHost, clock } = threeSeats();
  let seq = 0;
  // Play a while on the first host: inputs (some aimed at future ticks, still pending at the snapshot), drafts, bots.
  for (let k = 0; k < 120; k++) {
    seq++;
    for (const [id, name, msg] of script(k, seq)) oldHost.handle(/** @type {string} */ (id), /** @type {string} */ (name), msg);
    if (k === 119) oldHost.handle('g2', 'Cy', { t: 'in', acts: [[seq * 3 + 2, 2, oldHost.state.tick + 5]] });
    clock.t += TICK_MS;
    oldHost.tick();
  }
  seq++;
  const snap = overTheWire(snapshotEngine(oldHost));
  assert.equal(snap.v, SNAPSHOT_VERSION);
  assert.equal(snap.seats.length, 3);
  assert.ok(snap.seats.some((/** @type {any} */ x) => x.acts.length), 'an input not applied yet travels with the snapshot');
  // The successor (g1, by the rule) resumes; the old host's control copy plays on as if nothing happened, minus
  // the old host's own player (it left).
  assert.equal(successorFromSnapshot(snap, ['g1', 'g2']), 'g1');
  const clock2 = { t: 50_000 };
  const newHost = resumeEngine(snap, { instance: 'inst-1', hostId: 'g1', leaving: 'h1', send() {}, publish() {}, now: () => clock2.t });
  const h1 = /** @type {any} */ (oldHost.byUid.get('h1'));
  oldHost.arena.hostUid = 'g1';
  oldHost.arena.leave(h1, 'leave');
  oldHost.byUid.delete('h1');
  assert.equal(stateHash(newHost.state), stateHash(oldHost.state), 'the same game at the handoff');
  assert.equal(newHost.arena.hostUid, 'g1');
  for (let k = 120; k < 400; k++) {
    seq++;
    for (const [id, name, msg] of script(k, seq)) {
      if (id === 'h1') continue;
      oldHost.handle(/** @type {string} */ (id), /** @type {string} */ (name), msg);
      newHost.handle(/** @type {string} */ (id), /** @type {string} */ (name), msg);
    }
    clock.t += TICK_MS;
    clock2.t += TICK_MS;
    oldHost.tick();
    newHost.tick();
    assert.equal(stateHash(newHost.state), stateHash(oldHost.state), `tick ${oldHost.state.tick}`);
  }
  assert.ok(newHost.state.round.n >= 1);
  oldHost.close();
  newHost.close();
});

test('a snapshot resumes mid-run in Draft Night: the run, keepers and points carry on', () => {
  const { e, clock } = threeSeats('draft');
  e.handle('h1', 'Alice', { t: 'settings', id: 2, body: { roundMin: 0.5 } });
  for (let k = 0; k < 260; k++) {
    clock.t += TICK_MS;
    e.handle('g1', 'Bob', { t: 'hb', at: clock.t });
    e.handle('g2', 'Cy', { t: 'hb', at: clock.t });
    e.tick();
  }
  const md = /** @type {any} */ (e.state.md);
  assert.ok(md.leg >= 2, `round ${md.leg} of the run`);
  const snap = overTheWire(snapshotEngine(e));
  const next = resumeEngine(snap, { instance: 'inst-1', hostId: 'g1', leaving: 'h1', send() {}, publish() {}, now: () => 0 });
  assert.deepEqual(next.state.md, e.state.md);
  e.close();
  next.close();
});

test('HostHandoff over the loopback channel: the successor hosts, the other guest keeps its seat and player', async () => {
  const clock = { t: 1000 };
  const hub = createLoopbackHub({ hostId: 'h1' });
  const mk = (/** @type {string} */ id, /** @type {string} */ name, /** @type {any} */ engine) =>
    createSdkTransport({ channel: hub.join(id, name), now: () => clock.t, snapshotEveryTicks: 7, ...(engine ? { engine } : {}) });
  const host = mk('h1', 'Alice', { instance: 'inst-h', cfg: { targetHeads: 2 }, seed: 'lb' });
  const page = async (/** @type {any} */ t, /** @type {string} */ name) => {
    const world = new World();
    const joined = await t.join({ uid: name, name });
    assert.ok(joined.ok, String(joined.error));
    world.ingest(joined, clock.t);
    /** @type {any[]} */
    const replies = [];
    t.start((/** @type {any} */ r, /** @type {number} */ at) => (replies.push(r), world.ingest(r, at)), () => world.cursor());
    return { t, world, replies };
  };
  const ph = await page(host, 'Alice');
  const p1 = await page(mk('g1', 'Bob'), 'Bob');
  const p2 = await page(mk('g2', 'Cy'), 'Cy');
  await ph.t.play();
  await p1.t.play();
  const play2 = await p2.t.play();
  assert.equal(play2.ok, true);
  /** @param {any} t @param {number} n */
  const run = async (t, n) => {
    for (let i = 0; i < n; i++) {
      clock.t += TICK_MS;
      t.engine?.tick();
      await flush();
    }
  };
  await run(host, 20);
  assert.ok(p1.t.handoff.snapshot(), 'guests hold the newest snapshot');
  const before = /** @type {any} */ (p2.t.handoff.snapshot());
  assert.equal(p2.t.handoff.successor(['g1', 'g2']), 'g1');
  assert.equal(p1.t.handoff.successor(['g2', 'g1']), 'g1', 'both guests name the same successor');
  const pid2 = /** @type {any} */ (host.engine).byUid.get('g2').pid;

  // The host's page goes away without a word; the platform makes g1 the host and tells everyone.
  /** @type {any} */ (host.engine).close();
  host.stop();
  hub.setHost('g1');
  const r1 = await p1.t.handoff.hostChanged('g1', 'h1');
  const r2 = await p2.t.handoff.hostChanged('g1', 'h1');
  assert.deepEqual(r1, { ok: true, role: 'host' });
  assert.deepEqual(r2, { ok: true, role: 'guest' });
  await flush();
  const eng = /** @type {any} */ (p1.t.engine);
  assert.ok(eng, 'the successor runs the game now');
  assert.ok(eng.state.tick >= before.tick, 'resumed from the snapshot, not from scratch');
  assert.equal(eng.byUid.get('g2').pid, pid2, 'the guest got its own seat and player back');
  assert.equal(eng.byUid.has('h1'), false, 'the old host left');
  assert.ok(!eng.state.players.some((/** @type {any} */ p) => p.uid === 'h1'));
  await run(p1.t, 6);
  p2.t.input({ turns: [1] });
  await run(p1.t, 4);
  assert.equal(eng.state.players.find((/** @type {any} */ p) => p.id === pid2).dir, 1, "the guest's input reaches the new host");
  assert.equal(p2.world.recvTick, eng.state.tick, 'the guest follows the new host');
  assert.ok(!p2.replies.some((r) => r.error === 'host-left'), 'the guest never saw the game end');
  await p1.t.leave();
  await p2.t.leave();
});

test('with snapshots off (the default), a guest whose host leaves still ends at once', async () => {
  const clock = { t: 1000 };
  const hub = createLoopbackHub({ hostId: 'h1' });
  const hostCh = hub.join('h1', 'Alice');
  const host = createSdkTransport({ channel: hostCh, now: () => clock.t, engine: { instance: 'x', cfg: { targetHeads: 0 }, seed: 'x' } });
  await host.join({ uid: 'Alice', name: 'Alice' });
  const g = createSdkTransport({ channel: hub.join('g1', 'Bob'), now: () => clock.t });
  const w = new World();
  w.ingest(await g.join({ uid: 'Bob', name: 'Bob' }), clock.t);
  /** @type {any[]} */
  const replies = [];
  g.start((r) => replies.push(r), () => w.cursor());
  assert.equal(g.handoff.snapshot(), null);
  hostCh.close();
  await flush();
  assert.ok(replies.some((r) => r.error === 'host-left'));
  assert.deepEqual(await g.handoff.hostChanged('g1', 'h1'), { ok: false, role: 'guest', error: 'no-snapshot' });
  await host.leave();
  await g.leave();
});
