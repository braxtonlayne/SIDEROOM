// @ts-check
// The development stand-in channel (BroadcastChannel between tabs): the first member hosts, the second joins
// through it, and a game runs across the two exactly as over the SDK.
import assert from 'node:assert/strict';
import { createBroadcastChannel } from '../client/net/bc-channel.js';
import { createSdkTransport } from '../client/net/sdk-transport.js';
import { World } from '../client/world.js';

test('two tabs of one instance: the first hosts, the second plays through it', async () => {
  const instance = `t${Date.now()}`;
  const a = await createBroadcastChannel({ instance, self: { id: 'fake-a', name: 'Ann' } });
  const b = await createBroadcastChannel({ instance, self: { id: 'fake-b', name: 'Ben' } });
  try {
    assert.equal(a.isHost, true);
    assert.equal(b.isHost, false);
    assert.equal(b.hostId, 'fake-a');
    await new Promise((r) => setTimeout(r, 50));
    assert.deepEqual(a.members().map((m) => m.name), ['Ann', 'Ben']);

    const ta = createSdkTransport({ channel: a, engine: { instance, cfg: { targetHeads: 0 } } });
    const tb = createSdkTransport({ channel: b });
    const wa = new World();
    const wb = new World();
    const ja = await ta.join({ uid: 'fake-a', name: 'Ann' });
    wa.ingest(ja, 0);
    ta.start((r, at) => wa.ingest(r, at), () => wa.cursor());
    const jb = await tb.join({ uid: 'fake-b', name: 'Ben' });
    assert.ok(jb.ok && jb.full);
    wb.ingest(jb, 0);
    tb.start((r, at) => wb.ingest(r, at), () => wb.cursor());
    const p = await tb.play();
    assert.equal(p.ok, true);
    await new Promise((r) => setTimeout(r, 1200));
    assert.ok(wb.recvTick >= 5, `the guest saw ${wb.recvTick} ticks`);
    assert.equal(wb.roster.size, 1, 'the guest player is in the roster');
    await ta.leave();
  } finally {
    a.close();
    b.close();
  }
});
