// @ts-check
// Host/guest over the loopback channel: the host's page runs the game, a guest's page reaches it through the
// SDK-shaped channel (JSON bytes, async delivery). Ticks are driven by hand with a fake clock.
import assert from 'node:assert/strict';
import { createLoopbackHub } from '../client/net/loopback.js';
import { createSdkTransport } from '../client/net/sdk-transport.js';
import { World } from '../client/world.js';
import { MAX_PEOPLE } from '../host/engine.js';

const TICK_MS = 1000 / 7;
const flush = async () => {
  for (let i = 0; i < 6; i++) await new Promise((r) => setTimeout(r, 0));
};

/** A fake clock shared by the host's engine, and a way to make time pass tick by tick. */
function setup(opt = {}) {
  const clock = { t: 1000 };
  const hub = createLoopbackHub({ hostId: 'h1', ...opt });
  const hostCh = hub.join('h1', 'Alice');
  const host = createSdkTransport({ channel: hostCh, now: () => clock.t, engine: { instance: 'inst-abcdef', cfg: { targetHeads: 0 }, seed: 'seed-1' } });
  /** @param {string} id @param {string} name */
  const guest = (id, name) => createSdkTransport({ channel: hub.join(id, name), now: () => clock.t });
  /** Make n ticks pass on the host. */
  const run = async (n) => {
    for (let i = 0; i < n; i++) {
      clock.t += TICK_MS;
      host.engine?.tick();
      await flush();
    }
  };
  /** A page: a transport and the World it feeds, as main.js wires them. */
  const page = async (t, name) => {
    const world = new World();
    const joined = await t.join({ uid: name, name });
    assert.ok(joined.ok, String(joined.error));
    world.ingest(joined, clock.t);
    /** @type {any[]} */
    const replies = [];
    t.start((r, at) => {
      replies.push(r);
      world.ingest(r, at);
    }, () => world.cursor());
    return { t, world, joined, replies };
  };
  return { clock, hub, hostCh, host, guest, run, page };
}

test('a guest joins the host: a full snapshot, the config and the room, with the identity the SDK gave', async () => {
  const s = setup();
  const hp = await s.page(s.host, 'Alice');
  assert.equal(hp.joined.cfg.w > 0, true);
  const g = await s.page(s.guest('g1', 'Bob'), 'Bob');
  assert.ok(g.joined.full, 'a snapshot, as the first poll of the HTTP server answered');
  assert.equal(g.world.w, hp.world.w);
  assert.equal(g.joined.room.instance, true);
  assert.equal(g.joined.you.host, false);
  assert.equal(hp.joined.you.host, true, 'the launcher hosts');
  // The name comes from the SDK member, not from the message.
  const seats = s.host.engine.arena.gameInfo().people;
  assert.deepEqual(seats, ['Alice', 'Bob']);
  s.host.stop();
  await s.host.leave();
});

test('a hello cannot choose its own identity or name', async () => {
  const s = setup();
  await s.page(s.host, 'Alice');
  const ch = s.hub.join('g9', 'Real Name');
  ch.toHost({ t: 'hello', name: 'Imposter', uid: 'h1', from: 'h1' });
  await flush();
  assert.deepEqual(s.host.engine.arena.gameInfo().people, ['Alice', 'Real Name']);
  assert.equal(s.host.engine.byUid.size, 2);
  await s.host.leave();
});

test('both pages see the same game: heads, ticks, and the guest\'s own input', async () => {
  const s = setup();
  const hp = await s.page(s.host, 'Alice');
  const g = await s.page(s.guest('g1', 'Bob'), 'Bob');
  assert.equal((await hp.t.play()).ok, true);
  const pg = await g.t.play();
  assert.equal(pg.ok, true);
  await s.run(20);
  assert.equal(g.world.recvTick, s.host.engine.state.tick, 'the guest has the newest tick');
  assert.equal(hp.world.recvTick, s.host.engine.state.tick);
  assert.equal(g.world.contig, hp.world.contig, 'no gaps');
  assert.equal(g.world.you.pid, pg.pid);
  assert.equal(g.world.roster.size, 2);
  // Steer the guest's head: an input travels to the host and is acknowledged back.
  const before = g.world.you.ack?.[0] ?? 0;
  const seq = g.t.input({ turns: [1] });
  await s.run(3);
  assert.ok(g.world.you.ack[0] >= seq && seq > before, `acked ${g.world.you.ack} for seq ${seq}`);
  const me = s.host.engine.state.players.find((p) => p.id === pg.pid);
  assert.equal(me.dir, 1, 'the host applied the turn');
  await s.host.leave();
});

test('settings: only the host changes them; a restart reaches the guest as a new round', async () => {
  const s = setup();
  const hp = await s.page(s.host, 'Alice');
  const g = await s.page(s.guest('g1', 'Bob'), 'Bob');
  await s.run(2);
  assert.deepEqual(await g.t.settings({ mode: 'teams' }), { ok: false, error: 'not-host' });
  const res = await hp.t.settings({ bots: 3, skill: 'hard' });
  assert.equal(res.ok, true);
  await s.run(3);
  assert.equal(g.world.game.bots, 3);
  assert.equal(g.world.game.skill, 'hard');
  assert.equal(hp.world.game.bots, 3);
  await s.host.leave();
});

test('a lost frame is noticed and the guest asks for a snapshot', async () => {
  let n = 0;
  const s = setup({ drop: (kind) => kind === 'state' && ++n === 6 });
  const hp = await s.page(s.host, 'Alice');
  const g = await s.page(s.guest('g1', 'Bob'), 'Bob');
  await hp.t.play();
  await g.t.play();
  await s.run(12);
  assert.equal(s.hub.stats.dropped, 1);
  // The gap leaves the guest behind until its resync lands (frames after the hole queue, the snapshot jumps over it).
  s.clock.t += 2000;
  await s.run(12);
  assert.equal(g.world.contig, s.host.engine.state.tick, 'caught up');
  await s.host.leave();
});

test('the host leaving tells the guests the game is over', async () => {
  const s = setup();
  await s.page(s.host, 'Alice');
  const g = await s.page(s.guest('g1', 'Bob'), 'Bob');
  g.replies.length = 0;
  s.hostCh.close();
  await flush();
  assert.ok(g.replies.some((r) => r.ok === false && r.error === 'host-left'));
  await s.host.leave();
});

test('a guest who leaves the call loses the seat at once', async () => {
  const s = setup();
  await s.page(s.host, 'Alice');
  const ch = s.hub.join('g2', 'Cy');
  const g = await s.page(createSdkTransport({ channel: ch, now: () => s.clock.t }), 'Cy');
  await g.t.play();
  await s.run(2);
  assert.equal(s.host.engine.arena.seats.size, 2);
  assert.equal(s.host.engine.state.humans, 2);
  ch.close();
  await flush();
  assert.equal(s.host.engine.arena.seats.size, 1, 'only the host is left');
  assert.equal(s.host.engine.state.players.filter((p) => p.kind === 'human').length, 0, 'their player is gone');
  assert.equal(s.host.engine.byUid.size, 1);
  await s.host.leave();
});

test('kick: the removed guest is told once, cannot come back, and the others carry on', async () => {
  const s = setup();
  const hp = await s.page(s.host, 'Alice');
  const g = await s.page(s.guest('g1', 'Bob'), 'Bob');
  await s.run(1);
  const ref = s.host.engine.arena.gameInfo().seats.find((x) => x[1] === 'Bob')[0];
  assert.deepEqual(await g.t.host({ action: 'kick', to: ref }), { ok: false, error: 'not-host' });
  assert.deepEqual(await hp.t.host({ action: 'kick', to: ref }), { ok: true });
  await flush();
  assert.ok(g.replies.some((r) => r.error === 'kicked'));
  const again = await s.guest('g1', 'Bob').join({ uid: 'g1', name: 'Bob' });
  assert.deepEqual(again, { ok: false, error: 'banned' });
  await s.host.leave();
});

test('a room holds at most ' + MAX_PEOPLE + ' people', async () => {
  const s = setup();
  await s.page(s.host, 'Alice');
  for (let i = 1; i < MAX_PEOPLE; i++) s.hub.join('p' + i, 'P' + i).toHost({ t: 'hello' });
  await flush();
  assert.equal(s.host.engine.arena.seats.size, MAX_PEOPLE);
  const late = await s.guest('late', 'Late').join({ uid: 'late', name: 'Late' });
  assert.deepEqual(late, { ok: false, error: 'room-full' });
  await s.host.leave();
});

test('a member cannot flood the host', async () => {
  const s = setup();
  await s.page(s.host, 'Alice');
  s.host.engine.handle('f1', 'F', { t: 'hello' });
  const seat = s.host.engine.byUid.get('f1');
  for (let i = 1; i <= 500; i++) s.host.engine.handle('f1', 'F', { t: 'in', acts: [[i, 1]] });
  assert.ok(seat.inSeq <= 60, 'at most 60 messages a second were read: ' + seat.inSeq);
  await s.host.leave();
});

test('a seat the host timed out is told, and the page can say hello again', async () => {
  const s = setup();
  await s.page(s.host, 'Alice');
  const g = await s.page(s.guest('g1', 'Bob'), 'Bob');
  await s.run(1);
  s.host.engine.byUid.get('g1').lastSeen = s.clock.t - 20_000; // no message for 20 s
  await s.run(1); // the arena times the seat out
  assert.equal(s.host.engine.byUid.has('g1'), false);
  g.replies.length = 0;
  g.t.input({ turns: [0] });
  await flush();
  assert.ok(g.replies.some((r) => r.error === 'no-seat'));
  const back = await g.t.join({ uid: 'g1', name: 'Bob' });
  assert.equal(back.ok, true);
  await s.host.leave();
});
