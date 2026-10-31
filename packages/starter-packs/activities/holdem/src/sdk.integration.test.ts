// The table against the real Embedded App SDK data API (ACT-CH-001) and its loopback hub, through the
// `sdkTransport` adapter: this is the wiring the bundle uses inside SIDE ROOM.

import { LoopbackHub, SideRoomSDK } from '@sideroom/embedded-app-sdk';
import { describe, expect, it } from 'vitest';
import { seededRandom } from './cards';
import { PlayerClient } from './client';
import { HostController } from './host';
import { sdkTransport, type SdkLike } from './transport';

const noWindow = {
  parent: null,
  hostOrigin: '',
  addEventListener() {},
  removeEventListener() {},
  hostWindow: null,
};

function join(hub: LoopbackHub, id: string) {
  const transport = hub.connect(id);
  const sdk = new SideRoomSDK('holdem', { env: noWindow, transport });
  return sdkTransport(sdk as unknown as SdkLike, transport.self, transport.host);
}

describe('with the real SDK and its loopback hub', () => {
  it('plays a hand: hole cards reach only their owner, the relay sees opaque envelopes', () => {
    const hub = new LoopbackHub('ann');
    const annTp = join(hub, 'ann');
    const bobTp = join(hub, 'bob');
    const cyTp = join(hub, 'cy');
    const host = new HostController(annTp, { rnd: seededRandom(11) });
    const ann = new PlayerClient(annTp);
    const bob = new PlayerClient(bobTp);
    const cy = new PlayerClient(cyTp);
    host.start();
    for (const c of [ann, bob, cy]) c.start();

    ann.sit('Ann');
    bob.sit('Bob');
    cy.sit('Cy');
    expect(bob.snapshot().table?.seats.filter(Boolean)).toHaveLength(3);

    host.deal();
    const all = { ann, bob, cy };
    for (const c of Object.values(all)) expect(c.myCards()).toHaveLength(2);
    const dealt = host.current.hand!.holes;
    for (const [id, c] of Object.entries(all)) expect(c.myCards()).toEqual(dealt[id]);

    // The relay carried the hole cards inside `reply` envelopes addressed by member id.
    const replies = hub.relayed
      .map((r) => JSON.parse(new TextDecoder().decode(r.bytes)) as { t: string; to?: string })
      .filter((e) => e.t === 'reply');
    // The host's own cards are delivered locally and never touch the relay.
    expect(replies.map((e) => e.to).sort()).toEqual(['bob', 'cy']);

    // Play to the end by checking and calling.
    let guard = 0;
    while (!ann.snapshot().table!.hand!.over) {
      if (guard++ > 60) throw new Error('hand does not end');
      const t = ann.snapshot().table!;
      const who = all[t.hand!.toAct as keyof typeof all];
      who.act(t.hand!.legal!.canCheck ? { type: 'check' } : { type: 'call' });
    }
    const end = cy.snapshot().table!;
    expect(end.seats.reduce((s, x) => s + (x?.chips ?? 0), 0)).toBe(3000);
    expect(Object.keys(end.hand!.result!.reveals).length).toBeGreaterThan(1);
  });

  it('the public table of a full nine-seat table fits one data message', () => {
    const ids = Array.from({ length: 9 }, (_, i) => `member-${i}`);
    const hub = new LoopbackHub(ids[0] as string);
    const tps = ids.map((id) => join(hub, id));
    const host = new HostController(tps[0]!, { rnd: seededRandom(3) });
    const clients = tps.map((tp) => new PlayerClient(tp));
    host.start();
    clients.forEach((c) => c.start());
    clients.forEach((c) => c.sit('N'.repeat(32)));
    expect(() => host.deal()).not.toThrow(); // `state.set` throws TOO_LARGE past 16 KiB
    // Preflop all-in chaos for the biggest document: everyone shoves and then the hand runs out.
    let guard = 0;
    while (!clients[0]!.snapshot().table!.hand!.over && guard++ < 40) {
      const t = clients[0]!.snapshot().table!;
      const idx = ids.indexOf(t.hand!.toAct as string);
      clients[idx]!.act({ type: 'allin' });
    }
    expect(clients[0]!.snapshot().table!.hand!.over).toBe(true);
  });
});
