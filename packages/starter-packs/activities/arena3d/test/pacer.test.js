// @ts-check
// client/pacer.js: how many fixed ticks a page owes, in a visible tab and in a hidden tab whose timers the browser
// throttles. The host of a match must stay on time while hidden; a peer need not.
import assert from 'node:assert/strict';
import initJolt from 'jolt-physics/wasm-compat';
import { TickPacer } from '../client/pacer.js';
import { HostSession } from '../net/host.js';
import { PredictedClient } from '../net/client.js';
import { createLoopbackHub } from '../net/loopback.js';
import { World } from '../sim/world.js';
import { idleInput } from '../sim/input.js';

const TICK_MS = 1000 / 60;

test('a visible tab runs about one tick per 60 Hz frame and never spirals after a stall', () => {
  const p = new TickPacer();
  let total = 0;
  for (let i = 0; i <= 600; i++) total += p.due(i * (1000 / 60));
  assert.ok(Math.abs(total - 600) <= 1, `10 s of frames gave ${total} ticks`);
  const stalled = p.due(600 * TICK_MS + 3000);
  assert.equal(stalled, 8, 'a 3 s stall runs at most 8 ticks');
  assert.ok(p.dropped > 100, 'and the rest is given up, not queued');
  assert.equal(p.due(600 * TICK_MS + 3000 + 17), 1, 'the next frame is normal again');
});

test('a hidden tab whose timer fires once a second still gets 60 ticks a second', () => {
  const throttled = new TickPacer();
  const old = new TickPacer({ catchUpMs: 250, catchUpTicks: 8 }); // what the page did before: same cap as a visible frame
  let ticks = 0;
  let oldTicks = 0;
  for (let s = 0; s <= 30; s++) {
    ticks += throttled.due(s * 1000, true);
    oldTicks += old.due(s * 1000, true);
  }
  assert.ok(Math.abs(ticks - 1800) <= 2, `30 s hidden at 1 Hz: ${ticks} ticks (real time is 1800)`);
  assert.ok(oldTicks < 600, `the old cap fell far behind: ${oldTicks}`);
  assert.equal(throttled.dropped, 0);
});

test('intensive throttling (one timer a minute) is cut at 5 s of catch-up and says how far behind it fell', () => {
  const p = new TickPacer();
  p.due(0, true);
  const n = p.due(60_000, true);
  assert.equal(n, 300);
  assert.ok(p.dropped >= 3290, `dropped ${p.dropped} ticks (55 s)`);
});

test('a hidden host catching up on a 1 Hz timer keeps its peers on time and publishes one snapshot per burst', async () => {
  const J = await initJolt();
  const hub = createLoopbackHub({ hostId: 'h', latencyMs: 30, seed: 3 });
  const host = new HostSession({ channel: hub.join('h', 'Host'), world: new World(J, { seed: 1 }), name: 'Host' });
  const peer = new PredictedClient({ channel: hub.join('p', 'Peer'), world: new World(J, { seed: 0 }), name: 'Peer' });
  const pacer = new TickPacer();
  let peerClock = 0;
  const peerPacer = new TickPacer();
  // 20 s: the host's tab is hidden and its timer fires once a second; the peer is visible at 60 Hz.
  for (let step = 0; step <= 1200; step++) {
    const ms = step * TICK_MS;
    hub.pump(ms);
    if (step % 60 === 0) {
      const n = pacer.due(ms, true);
      const before = host.stats.snapshots;
      for (let i = 0; i < n; i++) host.tick(idleInput(), n <= 4);
      if (n > 4) host.publishSnapshot();
      if (n > 4) assert.equal(host.stats.snapshots - before, 1, 'one snapshot for a burst of 60 ticks');
    }
    peerClock = ms;
    for (let i = peerPacer.due(peerClock); i > 0; i--) peer.tick(idleInput());
  }
  assert.ok(Math.abs(host.world.tick - 1200) <= 62, `host tick ${host.world.tick} after 20 s (1200 on time)`);
  assert.ok(Math.abs(peer.hostTick - host.world.tick) <= 62, 'the peer follows within one burst');
  assert.equal(peer.status, 'playing');
});
