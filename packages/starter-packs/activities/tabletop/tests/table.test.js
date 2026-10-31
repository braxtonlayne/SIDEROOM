import { Fog } from '../src/fog.js';
import { verifyRecord } from '../src/dice.js';
import { K, applyWrites, handleInput, mapImageUrl, safeImageUrl, splitChunks, view } from '../src/table.js';
import { createLoopbackRoom, cryptoSeed, decodeInput, encodeInput, mayWrite, runHost } from '../src/transport.js';

const MAP = { kind: 'square', size: 50, ox: 0, oy: 0, width: 500, height: 400, unit: 5, unitName: 'ft' };
const env = (seed = 7n) => ({ hostId: 'host', now: 1000, seed: () => seed });
const run = (doc, from, msg, e = env()) => {
  const res = handleInput(doc, from, msg, e);
  applyWrites(doc, res.writes);
  return res;
};
const tok = (id, c = 0, r = 0) => ({ t: 'token-add', token: { id, name: id, c, r } });

describe('map and fog (host rules)', () => {
  it('only the host sets the map and the fog', () => {
    const doc = {};
    expect(run(doc, 'alice', { t: 'map', map: MAP }).error).toBe('host-only');
    expect(run(doc, 'host', { t: 'map', map: MAP }).error).toBeUndefined();
    expect(view(doc).map.size).toBe(50);
    const fog = new Fog(10, 8, true);
    expect(run(doc, 'alice', { t: 'fog', enc: fog.encode() }).error).toBe('host-only');
    expect(run(doc, 'host', { t: 'fog', enc: fog.encode() }).error).toBeUndefined();
    expect(view(doc).fog.hiddenCount()).toBe(80);
  });
  it('refuses a bad map and a fog of the wrong size', () => {
    const doc = {};
    expect(run(doc, 'host', { t: 'map', map: { ...MAP, size: 1 } }).error).toBe('bad-map');
    run(doc, 'host', { t: 'map', map: MAP });
    expect(run(doc, 'host', { t: 'fog', enc: new Fog(3, 3).encode() }).error).toBe('bad-fog');
    expect(run(doc, 'host', { t: 'fog', enc: 'junk' }).error).toBe('bad-fog');
  });
  it('clears the fog when the grid changes and pulls tokens back inside', () => {
    const doc = {};
    run(doc, 'host', { t: 'map', map: MAP });
    run(doc, 'host', { t: 'fog', enc: new Fog(10, 8, true).encode() });
    run(doc, 'host', tok('t1', 9, 7));
    run(doc, 'host', { t: 'map', map: { ...MAP, size: 100 } });
    expect(view(doc).fog).toBeNull();
    expect(doc[K.tok('t1')]).toMatchObject({ c: 4, r: 3 });
  });
  it('turns fog off with null', () => {
    const doc = {};
    run(doc, 'host', { t: 'map', map: MAP });
    run(doc, 'host', { t: 'fog', enc: new Fog(10, 8, true).encode() });
    run(doc, 'host', { t: 'fog', enc: null });
    expect(view(doc).fog).toBeNull();
  });
  it('reassembles a chunked map image and waits for every chunk', () => {
    const data = 'data:image/jpeg;base64,' + 'QUJD'.repeat(7000);
    const chunks = splitChunks(data);
    expect(chunks.length).toBeGreaterThan(2);
    const doc = {};
    run(doc, 'host', { t: 'map', map: { ...MAP, imgId: 'abc123', imgChunks: chunks.length } });
    expect(view(doc).image).toBeNull();
    chunks.forEach((c, i) => (doc[K.img('abc123', i)] = c));
    expect(view(doc).image).toBe(data);
    expect(mapImageUrl(doc, null)).toBeNull();
  });
});

describe('image URLs', () => {
  it('draws only http(s) and small base64 images', () => {
    expect(safeImageUrl('https://x.test/a.png')).toBe('https://x.test/a.png');
    expect(safeImageUrl('javascript:alert(1)')).toBe('');
    expect(safeImageUrl('data:text/html;base64,AAAA')).toBe('');
    expect(safeImageUrl('data:image/svg+xml;base64,AAAA')).toBe('');
    expect(safeImageUrl('data:image/png;base64,' + 'A'.repeat(20000))).toBe('');
    expect(safeImageUrl('data:image/png;base64,AAAA')).not.toBe('');
  });
});

describe('tokens', () => {
  const doc = {};
  run(doc, 'host', { t: 'map', map: MAP });

  it('a member adds one token of their own, the host any', () => {
    expect(run(doc, 'alice', tok('alice', 1, 1)).error).toBeUndefined();
    expect(doc[K.tok('alice')].owner).toBe('alice');
    expect(run(doc, 'alice', tok('alice2')).error).toBe('has-token');
    expect(run(doc, 'host', { t: 'token-add', token: { id: 'orc', name: 'Orc', c: 5, r: 5, hidden: true } }).error).toBeUndefined();
    expect(doc[K.tok('orc')]).toMatchObject({ owner: null, hidden: true });
    expect(run(doc, 'host', tok('orc')).error).toBe('bad-id');
    expect(run(doc, 'host', { t: 'token-add', token: { id: 'a b', name: 'x' } }).error).toBe('bad-id');
  });
  it('a member cannot add a hidden or someone else\'s token', () => {
    run(doc, 'bob', { t: 'token-add', token: { id: 'bob', name: 'Bob', owner: 'alice', hidden: true } });
    expect(doc[K.tok('bob')]).toMatchObject({ owner: 'bob', hidden: false });
  });
  it('moves: the owner and the host may, others may not', () => {
    expect(run(doc, 'alice', { t: 'move', id: 'alice', c: 3, r: 2 }).error).toBeUndefined();
    expect(doc[K.tok('alice')]).toMatchObject({ c: 3, r: 2 });
    expect(run(doc, 'bob', { t: 'move', id: 'alice', c: 4, r: 4 }).error).toBe('not-yours');
    expect(run(doc, 'alice', { t: 'move', id: 'orc', c: 4, r: 4 }).error).toBe('not-yours');
    expect(run(doc, 'host', { t: 'move', id: 'alice', c: 4, r: 4 }).error).toBeUndefined();
    expect(doc[K.tok('alice')]).toMatchObject({ c: 4, r: 4 });
  });
  it('refuses a move off the map or to a fractional cell, and a missing token', () => {
    expect(run(doc, 'alice', { t: 'move', id: 'alice', c: 10, r: 0 }).error).toBe('out-of-bounds');
    expect(run(doc, 'alice', { t: 'move', id: 'alice', c: 0.5, r: 0 }).error).toBe('out-of-bounds');
    expect(run(doc, 'alice', { t: 'move', id: 'ghost', c: 0, r: 0 }).error).toBe('no-token');
    expect(run(doc, 'alice', { t: 'move', id: 'alice', c: 4, r: 4 }).writes).toEqual([]);
  });
  it('only the host hides, renames or resizes a token', () => {
    expect(run(doc, 'alice', { t: 'token-set', id: 'alice', patch: { hidden: true } }).error).toBe('host-only');
    run(doc, 'host', { t: 'token-set', id: 'alice', patch: { hidden: true, size: 2, name: 'A' } });
    expect(doc[K.tok('alice')]).toMatchObject({ hidden: true, size: 2, name: 'A' });
  });
  it('removes a token and its initiative entry; only its owner or the host may', () => {
    run(doc, 'host', { t: 'init', op: 'add', entry: { id: 'e-orc', name: 'Orc', init: 5, tokenId: 'orc' } });
    expect(run(doc, 'bob', { t: 'token-del', id: 'orc' }).error).toBe('not-yours');
    run(doc, 'host', { t: 'token-del', id: 'orc' });
    expect(doc[K.tok('orc')]).toBeUndefined();
    expect(view(doc).tracker.entries).toHaveLength(0);
  });
  it('caps the token count and ignores tokens outside the map in the view', () => {
    const d = {};
    run(d, 'host', { t: 'map', map: MAP });
    for (let i = 0; i < 200; i++) run(d, 'host', tok(`t${i}`));
    expect(run(d, 'host', tok('one-more')).error).toBe('too-many');
    d[K.tok('stray')] = { id: 'stray', c: 99, r: 99 };
    expect(view(d).tokens.find((t) => t.id === 'stray')).toBeUndefined();
  });
  it('malformed intents are refused', () => {
    expect(run({}, 'host', null).error).toBe('malformed');
    expect(run({}, 'host', { t: 'nope' }).error).toBe('malformed');
    expect(run({}, 'host', { t: 5 }).error).toBe('malformed');
  });
});

describe('initiative through the host', () => {
  const doc = {};
  run(doc, 'host', { t: 'map', map: MAP });
  run(doc, 'alice', tok('alice'));
  run(doc, 'host', { t: 'init', op: 'add', entry: { id: 'e-alice', name: 'Alice', init: 12, owner: 'alice', tokenId: 'alice' } });
  run(doc, 'host', { t: 'init', op: 'add', entry: { id: 'e-orc', name: 'Orc', init: 9 } });

  it('only the host adds, removes, starts and steps back', () => {
    for (const op of ['add', 'remove', 'start', 'stop', 'prev']) {
      expect(run(doc, 'alice', { t: 'init', op, entry: { id: 'x', name: 'x' } }).error).toBe('host-only');
    }
  });
  it('the host starts; only the host or the member on turn ends a turn', () => {
    run(doc, 'host', { t: 'init', op: 'start' });
    expect(view(doc).tracker).toMatchObject({ turn: 'e-alice', round: 1 });
    expect(run(doc, 'bob', { t: 'init', op: 'next' }).error).toBe('not-your-turn');
    run(doc, 'alice', { t: 'init', op: 'next' });
    expect(view(doc).tracker.turn).toBe('e-orc');
    expect(run(doc, 'alice', { t: 'init', op: 'next' }).error).toBe('not-your-turn');
    run(doc, 'host', { t: 'init', op: 'next' });
    expect(view(doc).tracker).toMatchObject({ turn: 'e-alice', round: 2 });
  });
  it('a member sets their own score but cannot mark themselves out or touch others', () => {
    run(doc, 'alice', { t: 'init', op: 'update', entry: { id: 'e-alice', init: 3, out: true } });
    const e = view(doc).tracker.entries.find((x) => x.id === 'e-alice');
    expect(e.init).toBe(3);
    expect(e.out).toBe(false);
    expect(run(doc, 'alice', { t: 'init', op: 'update', entry: { id: 'e-orc', init: 99 } }).error).toBe('not-yours');
  });
});

describe('the roll record in shared state', () => {
  it('appends a verifiable record with the seed and a sequence number', () => {
    const doc = {};
    const res = run(doc, 'alice', { t: 'roll', expr: '2d20kh1+5', label: 'Stealth' }, env(0xabcdn));
    expect(res.error).toBeUndefined();
    expect(doc[K.rollSeq]).toBe(1);
    const rec = doc[K.roll(1)];
    expect(rec).toMatchObject({ seq: 1, by: 'alice', label: 'Stealth', expression: '2d20kh1+5', at: 1000 });
    expect(rec.seed).toBe('000000000000abcd');
    expect(verifyRecord(rec)).toBe(true);
    run(doc, 'bob', { t: 'roll', expr: 'd6' }, env(1n));
    expect(view(doc).rolls.map((r) => [r.seq, r.by])).toEqual([[1, 'alice'], [2, 'bob']]);
  });
  it('the same seed always gives the same dice, a different seed (nearly always) another', () => {
    const a = {};
    const b = {};
    run(a, 'x', { t: 'roll', expr: '10d100' }, env(5n));
    run(b, 'y', { t: 'roll', expr: '10d100' }, env(5n));
    expect(a[K.roll(1)].groups).toEqual(b[K.roll(1)].groups);
    const c = {};
    run(c, 'x', { t: 'roll', expr: '10d100' }, env(6n));
    expect(c[K.roll(1)].total).not.toBe(a[K.roll(1)].total);
  });
  it('refuses a bad expression without using a sequence number', () => {
    const doc = {};
    expect(run(doc, 'a', { t: 'roll', expr: '2d' }).error).toBe('syntax');
    expect(run(doc, 'a', { t: 'roll', expr: '0d6' }).error).toBe('zero-dice');
    expect(doc[K.rollSeq]).toBeUndefined();
  });
  it('keeps only the last 50 records', () => {
    const doc = {};
    for (let i = 0; i < 60; i++) run(doc, 'a', { t: 'roll', expr: 'd20' }, env(BigInt(i)));
    const seqs = view(doc).rolls.map((r) => r.seq);
    expect(seqs).toHaveLength(50);
    expect(seqs[0]).toBe(11);
    expect(seqs.at(-1)).toBe(60);
  });
  it('a tampered record in the document fails verification', () => {
    const doc = {};
    run(doc, 'a', { t: 'roll', expr: '3d6' }, env(9n));
    doc[K.roll(1)].total += 1;
    expect(verifyRecord(view(doc).rolls[0])).toBe(false);
  });
  it('rolls initiative: the host for anyone, a member only for their own entry', () => {
    const doc = {};
    run(doc, 'host', { t: 'map', map: MAP });
    run(doc, 'host', { t: 'init', op: 'add', entry: { id: 'e1', name: 'A', init: 0, owner: 'alice' } });
    run(doc, 'host', { t: 'init', op: 'add', entry: { id: 'e2', name: 'B', init: 0, owner: 'bob' } });
    run(doc, 'alice', { t: 'roll', expr: 'd20', initFor: 'e1' }, env(3n));
    const t = view(doc).tracker;
    expect(t.entries.find((e) => e.id === 'e1').init).toBe(doc[K.roll(1)].total);
    run(doc, 'alice', { t: 'roll', expr: 'd20+30', initFor: 'e2' }, env(4n));
    expect(view(doc).tracker.entries.find((e) => e.id === 'e2').init).toBe(0);
    run(doc, 'host', { t: 'roll', expr: 'd20+30', initFor: 'e2' }, env(4n));
    expect(view(doc).tracker.entries.find((e) => e.id === 'e2').init).toBeGreaterThan(30);
  });
});

describe('two participants over the loopback transport', () => {
  const setup = () => {
    const room = createLoopbackRoom('host');
    const host = room.join({ id: 'host', name: 'Hosta' });
    const alice = room.join({ id: 'alice', name: 'Alice' });
    runHost(host, handleInput, { seed: () => 11n, now: () => 5 });
    host.set(K.map, MAP);
    return { room, host, alice };
  };

  it('moves a token in one context and sees it in the other', () => {
    const { host, alice } = setup();
    let seen = 0;
    host.onChange(() => seen++);
    alice.send(tok('alice', 0, 0));
    alice.send({ t: 'move', id: 'alice', c: 6, r: 3 });
    expect(view(host.doc()).tokens.find((t) => t.id === 'alice')).toMatchObject({ c: 6, r: 3 });
    expect(view(alice.doc()).tokens.find((t) => t.id === 'alice')).toMatchObject({ c: 6, r: 3 });
    expect(seen).toBeGreaterThan(0);
  });
  it('enforces the SDK write rule: host:* is the host\'s, rulers are each member\'s own', () => {
    const { host, alice } = setup();
    expect(() => alice.set(K.map, MAP)).toThrow();
    expect(() => alice.set('host:tok:x', {})).toThrow();
    expect(() => alice.set(K.ruler('host'), {})).toThrow();
    alice.set(K.ruler('alice'), { a: { c: 0, r: 0 }, b: { c: 2, r: 2 } });
    expect(view(host.doc()).rulers.alice.b).toEqual({ c: 2, r: 2 });
    alice.set(K.ruler('alice'), null);
    expect(view(host.doc()).rulers.alice).toBeUndefined();
    expect(mayWrite('host', 'host', 'host:fog')).toBe(true);
    expect(mayWrite('alice', 'host', 'host:fog')).toBe(false);
  });
  it('rolls through the host and both see the same verifiable record', () => {
    const { host, alice } = setup();
    alice.send({ t: 'roll', expr: '4d6dl1', label: 'stats' });
    const a = view(alice.doc()).rolls;
    const h = view(host.doc()).rolls;
    expect(a).toHaveLength(1);
    expect(a).toEqual(h);
    expect(a[0].by).toBe('alice');
    expect(verifyRecord(a[0])).toBe(true);
  });
  it('rolls with the CSPRNG seed when none is injected (as the real Activity runs)', () => {
    const room = createLoopbackRoom('host');
    const host = room.join({ id: 'host', name: 'Hosta' });
    const alice = room.join({ id: 'alice', name: 'Alice' });
    runHost(host, handleInput);
    alice.send({ t: 'roll', expr: '4d6dl1' });
    alice.send({ t: 'roll', expr: '4d6dl1' });
    const rolls = view(host.doc()).rolls;
    expect(rolls).toHaveLength(2);
    expect(rolls[0].seed).toMatch(/^[0-9a-f]{16}$/);
    expect(rolls[0].seed).not.toBe(rolls[1].seed);
    expect(rolls.every(verifyRecord)).toBe(true);
    expect(typeof cryptoSeed()).toBe('bigint');
  });
  it('does not let a member impersonate another: identity comes from the channel, not the message', () => {
    const { host, alice } = setup();
    host.send(tok('hero', 1, 1));
    alice.send({ t: 'move', id: 'hero', c: 2, r: 2, from: 'host' });
    expect(view(host.doc()).tokens.find((t) => t.id === 'hero')).toMatchObject({ c: 1, r: 1 });
  });
  it('carries intents as bytes', () => {
    expect(decodeInput(encodeInput({ t: 'move', id: 'a', c: 1, r: 2 }))).toEqual({ t: 'move', id: 'a', c: 1, r: 2 });
    expect(decodeInput(new Uint8Array([1, 2, 3]))).toBeNull();
    expect(decodeInput(encodeInput(5))).toBeNull();
  });
});
