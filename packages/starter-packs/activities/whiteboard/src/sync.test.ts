import { describe, expect, it } from 'vitest';
import { LoopbackHub } from './loopback';
import { HOME_PAGE, type Payload } from './log';
import { Board, BASE_META } from './sync';

const stroke = (x = 0, pg = HOME_PAGE): Payload => ({
  k: 'stroke',
  pg,
  c: '#000000',
  w: 3,
  hl: false,
  pts: [x, 0, x + 5, 5],
});

function room(opts: { manual?: boolean; minAgeMs?: number; chunkChars?: number } = {}) {
  const hub = new LoopbackHub({ manual: opts.manual });
  let t = 0;
  const now = () => t;
  const mkBoard = (id: string, host: boolean) =>
    new Board({
      me: id,
      isHost: host,
      adapter: hub.join(id, host),
      compactDelayMs: 0,
      minAgeMs: opts.minAgeMs ?? 0,
      chunkChars: opts.chunkChars,
      now,
    });
  return {
    hub,
    host: mkBoard('host', true),
    alice: mkBoard('alice', false),
    bob: mkBoard('bob', false),
    tick: (ms: number) => (t += ms),
  };
}

const ids = (b: Board) => b.view().objects.map((o) => o.id);

describe('Board replication', () => {
  it('two participants converge on the same board', () => {
    const { host, alice, bob } = room();
    alice.add(stroke(1));
    bob.add(stroke(2));
    alice.add(stroke(3));
    expect(ids(alice)).toEqual(ids(bob));
    expect(ids(host)).toEqual(ids(bob));
    expect(ids(alice)).toHaveLength(3);
  });

  it('converges when changes arrive in a different order', () => {
    const { hub, alice, bob } = room({ manual: true });
    alice.add(stroke(1));
    bob.add(stroke(2));
    alice.add(stroke(3));
    hub.flush({ reverse: true });
    expect(ids(alice)).toEqual(ids(bob));
    expect(ids(alice)).toHaveLength(3);
  });

  it('a late joiner reads the whole document', () => {
    const { hub, alice } = room();
    alice.add(stroke(1));
    alice.add(stroke(2));
    const late = new Board({
      me: 'late',
      isHost: false,
      adapter: hub.join('late', false),
      compactDelayMs: 0,
    });
    expect(ids(late)).toEqual(ids(alice));
    late.add(stroke(3));
    expect(ids(alice)).toHaveLength(3);
  });

  it('undo and redo travel to the others', () => {
    const { alice, bob } = room();
    alice.add(stroke(1));
    alice.add(stroke(2));
    expect(bob.undo()).toBe(false);
    expect(alice.undo()).toBe(true);
    expect(ids(bob)).toHaveLength(1);
    expect(alice.redo()).toBe(true);
    expect(ids(bob)).toHaveLength(2);
    expect(alice.redo()).toBe(false);
  });

  it('ignores garbage written under op: keys', () => {
    const { hub, alice } = room();
    hub.join('mallory', false).set('op:alice:99', { a: 'alice', n: 99, ts: 1, p: { k: 'script' } });
    hub.join('mallory', false).set('op:alice:5', { a: 'bob', n: 5, ts: 1, p: stroke() });
    expect(ids(alice)).toHaveLength(0);
  });

  it('a non-host cannot write host keys', () => {
    const { hub, alice } = room();
    const mallory = hub.join('mallory', false);
    mallory.set(BASE_META, { gen: 9, chunks: 0, through: { alice: 99 } });
    expect(hub.refused).toEqual([BASE_META]);
    alice.add(stroke(1));
    expect(ids(alice)).toHaveLength(1);
  });
});

describe('compaction by the host', () => {
  it('only the host compacts', () => {
    const { alice } = room();
    alice.add(stroke());
    expect(alice.compact()).toBe(0);
  });

  it('folds the log, keeps the picture, deletes the folded keys', () => {
    const { hub, host, alice, bob } = room();
    for (let i = 0; i < 6; i++) alice.add(stroke(i));
    for (let i = 0; i < 4; i++) bob.add(stroke(100 + i));
    alice.undo();
    alice.undo();
    bob.add({ k: 'erase', t: ids(bob)[0] as string });
    const before = ids(bob);
    const folded = host.compact();
    expect(folded).toBeGreaterThan(0);
    expect(hub.keys.filter((k) => k.startsWith('op:'))).toEqual([]);
    expect(hub.keys.some((k) => k.startsWith('host:base:1:'))).toBe(true);
    expect(ids(host)).toEqual(before);
    expect(ids(alice)).toEqual(before);
    expect(ids(bob)).toEqual(before);
    expect(host.liveCount()).toBe(0);
    // The snapshot is smaller than the log: undone and erased entries are gone.
    expect(before).toHaveLength(7);
  });

  it('keeps working after a compaction: new strokes, undo of a compacted stroke, a second round', () => {
    const { hub, host, alice, bob } = room();
    alice.add(stroke(1));
    alice.add(stroke(2));
    host.compact();
    const c = alice.add(stroke(3));
    expect(ids(bob)).toHaveLength(3);
    expect(alice.undo()).toBe(true); // undoes stroke 3
    expect(ids(bob)).toHaveLength(2);
    expect(alice.undo()).toBe(true); // undoes a compacted stroke
    expect(ids(bob)).toHaveLength(1);
    expect(ids(bob)).not.toContain(c);
    host.compact();
    expect(hub.keys.some((k) => k.startsWith('host:base:1:'))).toBe(false); // the old chunks are gone
    expect(hub.keys.some((k) => k.startsWith('host:base:2:'))).toBe(true);
    expect(ids(alice)).toEqual(ids(bob));
    expect(ids(bob)).toHaveLength(1);
    // Counters continue past the compacted ones.
    const n = alice.add(stroke(9));
    expect(n).toBe('alice:6');
  });

  it('does not fold entries younger than the minimum age', () => {
    const { host, alice, tick } = room({ minAgeMs: 1000 });
    alice.add(stroke(1));
    tick(500);
    alice.add(stroke(2));
    expect(host.compact()).toBe(0);
    tick(600); // first is 1100 old, second 600
    expect(host.compact()).toBe(1);
    expect(host.liveCount()).toBe(1);
    expect(ids(alice)).toHaveLength(2);
  });

  it("folds only a contiguous run of an author's counters", () => {
    const { hub, host } = room();
    const w = hub.join('zed', false);
    w.set('op:zed:1', { a: 'zed', n: 1, ts: 1, p: stroke(1) });
    w.set('op:zed:3', { a: 'zed', n: 3, ts: 3, p: stroke(3) });
    expect(host.compact()).toBe(1);
    expect(host.liveCount()).toBe(1);
    w.set('op:zed:2', { a: 'zed', n: 2, ts: 2, p: stroke(2) });
    expect(host.compact()).toBe(2);
    expect(host.view().objects.map((o) => o.id)).toEqual(['zed:1', 'zed:2', 'zed:3']);
  });

  it('splits a big snapshot into chunks and a late joiner assembles them', () => {
    const { hub, host, alice } = room({ chunkChars: 200 });
    for (let i = 0; i < 12; i++) alice.add(stroke(i));
    host.compact();
    const chunkKeys = hub.keys.filter((k) => k.startsWith('host:base:1:'));
    expect(chunkKeys.length).toBeGreaterThan(3);
    const late = new Board({
      me: 'late',
      isHost: false,
      adapter: hub.join('late', false),
      compactDelayMs: 0,
    });
    expect(ids(late)).toEqual(ids(alice));
    expect(ids(late)).toHaveLength(12);
  });

  it('survives the deletions arriving before the snapshot (no stroke flickers away)', () => {
    const { hub, host, alice, bob } = room({ manual: true });
    for (let i = 0; i < 3; i++) alice.add(stroke(i));
    hub.flush();
    expect(ids(bob)).toHaveLength(3);
    host.compact();
    hub.flush({ reverse: true }); // deletions first, then meta, then chunks
    expect(ids(bob)).toHaveLength(3);
    expect(ids(bob)).toEqual(ids(alice));
  });

  it('shows the previous snapshot until all chunks of the new one have arrived', () => {
    const { hub, host, alice, bob } = room({ manual: true, chunkChars: 150 });
    for (let i = 0; i < 8; i++) alice.add(stroke(i));
    hub.flush();
    host.compact();
    // Deliver only the header and the deletions: chunks are missing.
    const full = hub.get(BASE_META);
    expect(full).toBeTruthy();
    // bob has not seen anything of the compaction yet, so the board is intact.
    expect(ids(bob)).toHaveLength(8);
    hub.flush();
    expect(ids(bob)).toHaveLength(8);
    expect(bob.compactions).toBe(1);
  });

  it('a compaction scheduled by the timer runs once the threshold is reached', async () => {
    const hub = new LoopbackHub();
    const host = new Board({
      me: 'h',
      isHost: true,
      adapter: hub.join('h', true),
      compactAt: 5,
      minAgeMs: 0,
      compactDelayMs: 5,
    });
    const alice = new Board({ me: 'a', isHost: false, adapter: hub.join('a', false) });
    for (let i = 0; i < 6; i++) alice.add(stroke(i));
    await new Promise((r) => setTimeout(r, 40));
    expect(host.compactions).toBe(1);
    expect(host.liveCount()).toBe(0);
    expect(ids(alice)).toHaveLength(6);
    host.close();
  });

  it('a long session stays bounded: undone and erased work does not accumulate', () => {
    const { hub, host, alice } = room();
    for (let round = 0; round < 5; round++) {
      for (let i = 0; i < 20; i++) alice.add(stroke(i));
      for (let i = 0; i < 20; i++) alice.undo();
      host.compact();
    }
    expect(ids(alice)).toHaveLength(0);
    expect(hub.keys.filter((k) => k.startsWith('op:'))).toEqual([]);
    expect(JSON.stringify(hub.get('host:base:5:0') ?? []).length).toBeLessThan(10); // an empty board
  });
});
