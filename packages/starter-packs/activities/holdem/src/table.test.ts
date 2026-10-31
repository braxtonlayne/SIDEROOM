import { describe, expect, it } from 'vitest';
import { seededRandom } from './cards';
import { PlayerClient } from './client';
import { HostController } from './host';
import { decodeClientMsg, decodeHostMsg, encode, TABLE_KEY } from './protocol';
import { LoopbackHub } from './transport';

const dec = new TextDecoder();

function setup(ids: string[], seed = 5) {
  const hub = new LoopbackHub(ids[0] as string);
  const hostTp = hub.connect(ids[0] as string);
  const host = new HostController(hostTp, { rnd: seededRandom(seed) });
  const clients = new Map<string, PlayerClient>();
  for (const id of ids) {
    const tp = id === ids[0] ? hostTp : hub.connect(id);
    const c = new PlayerClient(tp);
    clients.set(id, c);
  }
  host.start();
  for (const c of clients.values()) c.start();
  hub.flush();
  const get = (id: string) => clients.get(id) as PlayerClient;
  return { hub, host, get };
}

describe('the dealer and the players over the data channel', () => {
  it('seats players, deals, and sends each hole hand to its owner only', () => {
    const { hub, host, get } = setup(['ann', 'bob', 'cy']);
    get('ann').sit('Ann');
    get('bob').sit('Bob');
    get('cy').sit('Cy');
    hub.flush();
    expect(get('cy').snapshot().table?.seats.filter(Boolean)).toHaveLength(3);
    expect(get('ann').snapshot().table?.canStart).toBe(true);

    host.deal();
    hub.flush();

    // One addressed reply per player, each to its owner.
    const holes = hub.replies.filter((r) => decodeHostMsg(r.data)?.t === 'hole');
    expect(holes.map((r) => r.to).sort()).toEqual(['ann', 'bob', 'cy']);
    const dealt = new Map<string, string[]>();
    for (const r of holes) {
      const m = decodeHostMsg(r.data);
      if (m?.t === 'hole') dealt.set(r.to, m.cards);
    }
    for (const id of ['ann', 'bob', 'cy']) {
      expect(get(id).myCards()).toEqual(dealt.get(id));
    }

    // Nothing the document ever held contains anybody's hole cards or the deck.
    const hand = host.current.hand!;
    const secrets = Object.values(hand.holes).flat();
    for (const written of hub.stateWrites) {
      for (const c of secrets) expect(written).not.toContain(`"${c}"`);
      expect(written).not.toContain('"deck"');
      expect(written).not.toContain('"holes"');
    }
  });

  it('plays a whole hand through the clients and reveals only at a contested showdown', () => {
    const { hub, host, get } = setup(['ann', 'bob']);
    get('ann').sit('Ann');
    get('bob').sit('Bob');
    hub.flush();
    host.deal();
    hub.flush();

    const turn = () => get('ann').snapshot().table!.hand!.toAct as string;
    let guard = 0;
    while (!get('ann').snapshot().table!.hand!.over) {
      if (guard++ > 40) throw new Error('no end');
      const who = turn();
      const legal = get(who).snapshot().table!.hand!.legal!;
      get(who).act(legal.canCheck ? { type: 'check' } : { type: 'call' });
      hub.flush();
    }
    const t = get('bob').snapshot().table!;
    expect(t.hand!.board).toHaveLength(5);
    expect(Object.keys(t.hand!.result!.reveals).sort()).toEqual(['ann', 'bob']);
    expect(t.seats.reduce((s, x) => s + (x?.chips ?? 0), 0)).toBe(2000);
    // The reveal matches the cards each player was sent.
    const annHole = get('ann').myCards();
    expect(t.hand!.result!.reveals.ann!.cards).toEqual(annHole);
  });

  it('a fold-out shows nobody\'s cards', () => {
    const { hub, host, get } = setup(['ann', 'bob', 'cy']);
    for (const id of ['ann', 'bob', 'cy']) get(id).sit(id);
    hub.flush();
    host.deal();
    hub.flush();
    const t0 = get('ann').snapshot().table!;
    get(t0.hand!.toAct!).act({ type: 'fold' });
    hub.flush();
    const t1 = get('ann').snapshot().table!;
    get(t1.hand!.toAct!).act({ type: 'fold' });
    hub.flush();
    const t2 = get('ann').snapshot().table!;
    expect(t2.hand!.over).toBe(true);
    expect(t2.hand!.result!.reveals).toEqual({});
    for (const w of hub.stateWrites) {
      for (const c of Object.values(host.current.hand!.holes).flat()) {
        expect(w).not.toContain(`"${c}"`);
      }
    }
  });

  it('refuses out-of-turn, stale and spoofed actions and tells only the sender', () => {
    const { hub, host, get } = setup(['ann', 'bob', 'cy']);
    for (const id of ['ann', 'bob', 'cy']) get(id).sit(id);
    hub.flush();
    host.deal();
    hub.flush();
    const t = get('ann').snapshot().table!;
    const waiting = ['ann', 'bob', 'cy'].find((id) => id !== t.hand!.toAct)!;
    const before = hub.stateWrites.length;
    get(waiting).act({ type: 'fold' });
    hub.flush();
    expect(get(waiting).snapshot().error).toMatch(/not your turn/i);
    expect(hub.stateWrites.length).toBe(before); // nothing changed
    const lastReply = hub.replies[hub.replies.length - 1]!;
    expect(lastReply.to).toBe(waiting);

    // A stale hand number is refused.
    host.handle(t.hand!.toAct!, { t: 'act', hand: 99, action: { type: 'fold' } });
    expect(hub.replies[hub.replies.length - 1]!.to).toBe(t.hand!.toAct);
    expect(host.current.hand!.players.every((p) => !p.folded)).toBe(true);
  });

  it('only the host can start a hand or write the table', () => {
    const { hub } = setup(['ann', 'bob']);
    const guest = hub.connect('mallory');
    expect(() => guest.setState({ [TABLE_KEY]: { forged: true } })).toThrow();
    expect(() => guest.reply('bob', encode({ t: 'hole', hand: 1, cards: ['As', 'Ad'] }))).toThrow();
  });

  it('a player whose message was missed gets their cards again on sync', () => {
    const { hub, host, get } = setup(['ann', 'bob']);
    get('ann').sit('Ann');
    get('bob').sit('Bob');
    hub.flush();
    host.deal();
    hub.flush();
    const cards = get('bob').myCards();
    expect(cards).not.toBeNull();
    // A fresh client for bob (a reload) learns the table from state and asks for the cards.
    const tp = hub.connect('bob');
    const reborn = new PlayerClient(tp);
    reborn.start();
    hub.flush();
    expect(reborn.myCards()).toEqual(cards);
  });

  it('a late joiner who is not dealt in is never sent cards', () => {
    const { hub, host, get } = setup(['ann', 'bob', 'dee']);
    get('ann').sit('Ann');
    get('bob').sit('Bob');
    hub.flush();
    host.deal();
    hub.flush();
    get('dee').sit('Dee'); // sits mid-hand: joins the next one
    get('dee').sync();
    hub.flush();
    expect(get('dee').myCards()).toBeNull();
    expect(hub.replies.filter((r) => r.to === 'dee' && decodeHostMsg(r.data)?.t === 'hole')).toEqual([]);
    expect(get('dee').snapshot().table!.seats.filter(Boolean)).toHaveLength(3);
  });

  it('the host changes blinds only between hands and a leaver is removed', () => {
    const { hub, host, get } = setup(['ann', 'bob', 'cy']);
    for (const id of ['ann', 'bob', 'cy']) get(id).sit(id);
    hub.flush();
    host.setBlinds(25, 50);
    hub.flush();
    expect(get('bob').snapshot().table!.config.bigBlind).toBe(50);
    host.deal();
    host.setBlinds(5, 10);
    hub.flush();
    expect(get('ann').snapshot().error).toMatch(/between hands/i);
    host.kick('cy');
    hub.flush();
    expect(get('ann').snapshot().table!.seats.find((s) => s?.id === 'cy')?.leaving).toBe(true);
  });

  it('keeps the shared document small enough for one data message', () => {
    const { hub, host, get } = setup(['a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7', 'a8', 'a9']);
    for (const id of ['a1', 'a2', 'a3', 'a4', 'a5', 'a6', 'a7', 'a8', 'a9']) get(id).sit('Player '.padEnd(30, 'x'));
    hub.flush();
    host.deal();
    hub.flush();
    for (const w of hub.stateWrites) expect(new TextEncoder().encode(w).length).toBeLessThan(16 * 1024);
  });
});

describe('message validation', () => {
  it('drops malformed client messages', () => {
    const enc = (v: unknown) => new TextEncoder().encode(JSON.stringify(v));
    expect(decodeClientMsg(enc({ t: 'act', hand: 1, action: { type: 'raise', to: 'lots' } }))).toBeNull();
    expect(decodeClientMsg(enc({ t: 'act', hand: 1, action: { type: 'steal' } }))).toBeNull();
    expect(decodeClientMsg(enc({ t: 'sit' }))).toBeNull();
    expect(decodeClientMsg(enc([1, 2]))).toBeNull();
    expect(decodeClientMsg(new Uint8Array([1, 2, 3]))).toBeNull();
    expect(decodeClientMsg(enc({ t: 'act', hand: 2, action: { type: 'raise', to: 80 } }))).toEqual({
      t: 'act',
      hand: 2,
      action: { type: 'raise', to: 80 },
    });
  });

  it('accepts only real cards in a hole message', () => {
    const enc = (v: unknown) => new TextEncoder().encode(JSON.stringify(v));
    expect(decodeHostMsg(enc({ t: 'hole', hand: 1, cards: ['As', 'Zz'] }))).toBeNull();
    expect(decodeHostMsg(enc({ t: 'hole', hand: 1, cards: ['As', 'Kd'] }))?.t).toBe('hole');
    expect(dec.decode(encode({ t: 'error', code: 'x', message: 'y' }))).toContain('"error"');
  });
});
