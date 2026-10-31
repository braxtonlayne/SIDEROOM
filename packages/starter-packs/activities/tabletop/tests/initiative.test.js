import * as I from '../src/initiative.js';

const build = () => {
  let t = I.emptyTracker();
  t = I.addEntry(t, { id: 'a', name: 'Aria', init: 15 });
  t = I.addEntry(t, { id: 'b', name: 'Bram', init: 20 });
  t = I.addEntry(t, { id: 'c', name: 'Cass', init: 15, tie: 3 });
  t = I.addEntry(t, { id: 'd', name: 'Dun', init: 8 });
  return t;
};
const ids = (t) => I.ordered(t).map((e) => e.id);

describe('initiative order', () => {
  it('sorts by score, then tie-break, then entry order', () => {
    expect(ids(build())).toEqual(['b', 'c', 'a', 'd']);
  });
  it('refuses duplicates, empty ids and more than the cap', () => {
    const t = build();
    expect(I.addEntry(t, { id: 'a', name: 'again' })).toBe(t);
    expect(I.addEntry(t, { id: '', name: 'x' })).toBe(t);
    let big = I.emptyTracker();
    for (let i = 0; i < I.MAX_ENTRIES + 5; i++) big = I.addEntry(big, { id: `e${i}`, name: 'x', init: i });
    expect(big.entries).toHaveLength(I.MAX_ENTRIES);
  });
  it('never mutates its input', () => {
    const t = build();
    const snap = JSON.stringify(t);
    I.start(t);
    I.updateEntry(t, 'a', { init: 99 });
    I.removeEntry(t, 'a');
    expect(JSON.stringify(t)).toBe(snap);
  });
  it('re-sorts when a score changes', () => {
    const t = I.updateEntry(build(), 'd', { init: 30 });
    expect(ids(t)[0]).toBe('d');
  });
});

describe('turns and rounds', () => {
  it('starts at the top of round 1', () => {
    const t = I.start(build());
    expect(t.turn).toBe('b');
    expect(t.round).toBe(1);
  });
  it('does not start an empty or all-out fight', () => {
    expect(I.start(I.emptyTracker()).turn).toBeNull();
    let t = build();
    for (const e of t.entries) t = I.updateEntry(t, e.id, { out: true });
    expect(I.start(t).turn).toBeNull();
  });
  it('walks the order and wraps into the next round', () => {
    let t = I.start(build());
    const seen = [];
    for (let i = 0; i < 5; i++) {
      seen.push(`${t.round}${t.turn}`);
      t = I.next(t);
    }
    expect(seen).toEqual(['1b', '1c', '1a', '1d', '2b']);
  });
  it('skips entries that are out, including when the current one goes out', () => {
    let t = I.start(build());
    t = I.updateEntry(t, 'c', { out: true });
    expect(I.next(t).turn).toBe('a');
    t = I.next(t); // a
    t = I.updateEntry(t, 'a', { out: true });
    expect(I.next(t).turn).toBe('d');
    // the last live entry in order goes out on its own turn: wrap to the first
    t = I.updateEntry({ ...t, turn: 'd' }, 'd', { out: true });
    const n = I.next(t);
    expect(n.turn).toBe('b');
    expect(n.round).toBe(2);
  });
  it('steps back, across a round boundary, and stops at the start', () => {
    let t = I.start(build());
    expect(I.previous(t)).toEqual(t);
    t = I.next(t);
    expect(I.previous(t).turn).toBe('b');
    t = { ...I.start(build()), round: 2 };
    const p = I.previous(t);
    expect(p.turn).toBe('d');
    expect(p.round).toBe(1);
  });
  it('moves the turn on when the current entry is removed', () => {
    let t = I.start(build());
    t = I.removeEntry(t, 'b');
    expect(t.turn).toBe('c');
    t = I.removeEntry(I.removeEntry(I.removeEntry(t, 'c'), 'a'), 'd');
    expect(t.turn).toBeNull();
    expect(t.round).toBe(0);
  });
  it('stops combat but keeps the scores', () => {
    const t = I.stop(I.start(build()));
    expect(t.turn).toBeNull();
    expect(t.round).toBe(0);
    expect(t.entries).toHaveLength(4);
  });
  it('does nothing without a turn', () => {
    const t = build();
    expect(I.next(t)).toBe(t);
    expect(I.previous(t)).toBe(t);
  });
});

describe('sanitize', () => {
  it('drops garbage and fixes a dangling turn', () => {
    expect(I.sanitize(null)).toEqual(I.emptyTracker());
    expect(I.sanitize({ entries: 'x' })).toEqual(I.emptyTracker());
    const t = I.sanitize({
      entries: [{ id: 'a', name: 'A', init: 9 }, 5, null, { id: 'b', name: 'B', init: 4, out: true }],
      turn: 'zzz',
      round: 3,
    });
    expect(t.entries.map((e) => e.id)).toEqual(['a', 'b']);
    expect(t.entries[1].out).toBe(true);
    expect(t.turn).toBeNull();
    expect(t.round).toBe(0);
    const ok = I.sanitize({ entries: [{ id: 'a', name: 'A', init: 4 }], turn: 'a', round: 0 });
    expect(ok.round).toBe(1);
  });
});
