import { describe, expect, it } from 'vitest';
import {
  HOME_PAGE,
  compactEntries,
  entryId,
  fold,
  parseEntry,
  redoTarget,
  sameView,
  undoTarget,
  type Entry,
  type Payload,
} from './log';

let ts = 0;
const counters = new Map<string, number>();
function mk(a: string, p: Payload, at?: number): Entry {
  const n = (counters.get(a) ?? 0) + 1;
  counters.set(a, n);
  ts += 1;
  return { a, n, ts: at ?? ts, p };
}
const stroke = (pg = HOME_PAGE, c = '#112233'): Payload => ({
  k: 'stroke',
  pg,
  c,
  w: 4,
  hl: false,
  pts: [0, 0, 10, 10],
});

describe('fold', () => {
  it('shows strokes in the order of the clock, then author, then counter', () => {
    counters.clear();
    ts = 0;
    const b = mk('bob', stroke(), 5);
    const a = mk('alice', stroke(), 5);
    const c = mk('alice', stroke(), 3);
    const ids = fold([b, a, c]).view.objects.map((o) => o.id);
    expect(ids).toEqual([entryId(c), entryId(a), entryId(b)]);
  });

  it('is independent of arrival order and ignores duplicates', () => {
    counters.clear();
    ts = 0;
    const es = [mk('a', stroke()), mk('b', stroke()), mk('a', stroke()), mk('b', stroke())];
    const x = fold(es).view;
    const y = fold([...es].reverse().concat(es[0] as Entry)).view;
    expect(sameView(x, y)).toBe(true);
  });

  it("undo hides the author's own entry and redo brings it back", () => {
    counters.clear();
    const s = mk('a', stroke());
    const u = mk('a', { k: 'undo', t: entryId(s) });
    expect(fold([s, u]).view.objects).toHaveLength(0);
    const r = mk('a', { k: 'redo', t: entryId(s) });
    expect(fold([s, u, r]).view.objects).toHaveLength(1);
  });

  it("refuses to undo someone else's entry", () => {
    counters.clear();
    const s = mk('a', stroke());
    const u = mk('b', { k: 'undo', t: entryId(s) });
    expect(fold([s, u]).view.objects).toHaveLength(1);
  });

  it('erase removes any object and cannot be undone away', () => {
    counters.clear();
    const s = mk('a', stroke());
    const e = mk('b', { k: 'erase', t: entryId(s) });
    const u = mk('b', { k: 'undo', t: entryId(e) });
    expect(fold([s, e, u]).view.objects).toHaveLength(0);
  });

  it('clear removes earlier objects on its page only, and undoing the clear restores them', () => {
    counters.clear();
    const s1 = mk('a', stroke());
    const page = mk('a', { k: 'page', name: 'Two' });
    const s2 = mk('a', stroke(entryId(page)));
    const clr = mk('b', { k: 'clear', pg: HOME_PAGE });
    const s3 = mk('a', stroke());
    const f = fold([s1, page, s2, clr, s3]);
    expect(f.view.objects.map((o) => o.id)).toEqual([entryId(s2), entryId(s3)]);
    const undo = mk('b', { k: 'undo', t: entryId(clr) });
    expect(fold([s1, page, s2, clr, s3, undo]).view.objects).toHaveLength(3);
  });

  it('pages: add, rename, delete; the last page stays', () => {
    counters.clear();
    const add = mk('a', { k: 'page', name: 'Sketch' });
    const ren = mk('b', { k: 'rename', t: entryId(add), name: 'Plan' });
    let f = fold([add, ren]);
    expect(f.view.pages).toEqual([
      { id: HOME_PAGE, name: 'Page 1' },
      { id: entryId(add), name: 'Plan' },
    ]);
    const delHome = mk('a', { k: 'pagedel', t: HOME_PAGE });
    f = fold([add, ren, delHome]);
    expect(f.view.pages.map((p) => p.id)).toEqual([entryId(add)]);
    const delLast = mk('a', { k: 'pagedel', t: entryId(add) });
    f = fold([add, ren, delHome, delLast]);
    expect(f.view.pages.map((p) => p.id)).toEqual([entryId(add)]);
  });

  it('objects on a deleted page disappear', () => {
    counters.clear();
    const add = mk('a', { k: 'page', name: 'x' });
    const s = mk('a', stroke(entryId(add)));
    const del = mk('a', { k: 'pagedel', t: entryId(add) });
    expect(fold([add, s, del]).view.objects).toHaveLength(0);
  });
});

describe('undo and redo targets', () => {
  it("picks the author's latest live entry, and redo the latest undone", () => {
    counters.clear();
    const s1 = mk('a', stroke());
    const s2 = mk('a', stroke());
    const other = mk('b', stroke());
    expect(undoTarget([s1, s2, other], 'a')).toBe(entryId(s2));
    const u = mk('a', { k: 'undo', t: entryId(s2) });
    expect(undoTarget([s1, s2, other, u], 'a')).toBe(entryId(s1));
    expect(redoTarget([s1, s2, other, u], 'a')).toBe(entryId(s2));
    expect(redoTarget([s1, s2, other, u], 'b')).toBeNull();
    expect(undoTarget([], 'a')).toBeNull();
  });

  it('can undo a clear', () => {
    counters.clear();
    const s = mk('a', stroke());
    const clr = mk('a', { k: 'clear', pg: HOME_PAGE });
    expect(undoTarget([s, clr], 'a')).toBe(entryId(clr));
  });
});

describe('compaction', () => {
  it('keeps what is visible and drops everything else', () => {
    counters.clear();
    ts = 0;
    const keep1 = mk('a', stroke());
    const gone = mk('a', stroke());
    const undoGone = mk('a', { k: 'undo', t: entryId(gone) });
    const erased = mk('b', stroke());
    const erase = mk('a', { k: 'erase', t: entryId(erased) });
    const clearedOld = mk('b', stroke());
    const clr = mk('a', { k: 'clear', pg: HOME_PAGE });
    const keep2 = mk('b', stroke());
    const all = [keep1, gone, undoGone, erased, erase, clearedOld, clr, keep2];
    const kept = compactEntries(all);
    expect(kept.map(entryId)).toEqual([entryId(keep2)]);
    expect(sameView(fold(kept).view, fold(all).view)).toBe(true);
  });

  it('keeps pages, their last name and a home deletion', () => {
    counters.clear();
    const add = mk('a', { k: 'page', name: 'one' });
    const r1 = mk('a', { k: 'rename', t: entryId(add), name: 'two' });
    const r2 = mk('b', { k: 'rename', t: entryId(add), name: 'three' });
    const del = mk('a', { k: 'pagedel', t: HOME_PAGE });
    const s = mk('a', stroke(entryId(add)));
    const all = [add, r1, r2, del, s];
    const kept = compactEntries(all);
    expect(kept.map(entryId).sort()).toEqual([add, r2, del, s].map(entryId).sort());
    expect(sameView(fold(kept).view, fold(all).view)).toBe(true);
  });

  it('is idempotent', () => {
    counters.clear();
    const es = [mk('a', stroke()), mk('b', stroke()), mk('a', { k: 'page', name: 'p' })];
    const once = compactEntries(es);
    expect(compactEntries(once).map(entryId)).toEqual(once.map(entryId));
  });

  it('an undo that arrives after its target was compacted does no harm', () => {
    counters.clear();
    const s = mk('a', stroke());
    const base = compactEntries([s]);
    const late = mk('a', { k: 'undo', t: entryId(s) });
    // The target is in the base, so the late undo still applies.
    expect(fold([...base, late]).view.objects).toHaveLength(0);
    // A late undo of an entry that was dropped is a no-op.
    const dead = mk('a', stroke());
    const undo = mk('a', { k: 'undo', t: entryId(dead) });
    const base2 = compactEntries([dead, undo]);
    const lateRedo = mk('a', { k: 'redo', t: entryId(dead) });
    expect(fold([...base2, lateRedo]).view.objects).toHaveLength(0);
  });
});

describe('parseEntry', () => {
  const good = { a: 'u1', n: 1, ts: 1, p: stroke() };

  it('accepts a good entry and rounds coordinates', () => {
    const e = parseEntry({ ...good, p: { ...stroke(), pts: [0.4, 1.6, 10, 10] } });
    expect(e?.p.k === 'stroke' && e.p.pts).toEqual([0, 2, 10, 10]);
  });

  it.each([
    ['null', null],
    ['a string', 'x'],
    ['bad counter', { ...good, n: 0 }],
    ['bad author', { ...good, a: 'a b' }],
    ['unknown kind', { ...good, p: { k: 'script' } }],
    ['bad colour', { ...good, p: { ...stroke(), c: 'red' } }],
    ['odd point list', { ...good, p: { ...stroke(), pts: [1, 2, 3] } }],
    ['NaN point', { ...good, p: { ...stroke(), pts: [1, NaN, 3, 4] } }],
    ['too many points', { ...good, p: { ...stroke(), pts: new Array(1402).fill(1) } }],
    [
      'javascript image',
      {
        ...good,
        p: { k: 'image', pg: 'home', src: 'javascript:alert(1)', at: [0, 0], size: [5, 5] },
      },
    ],
    [
      'http image',
      { ...good, p: { k: 'image', pg: 'home', src: 'http://x/y.png', at: [0, 0], size: [5, 5] } },
    ],
    [
      'svg data image',
      {
        ...good,
        p: {
          k: 'image',
          pg: 'home',
          src: 'data:image/svg+xml;base64,AAAA',
          at: [0, 0],
          size: [5, 5],
        },
      },
    ],
    [
      'empty text',
      { ...good, p: { k: 'text', pg: 'home', c: '#000000', size: 20, at: [0, 0], text: '  ' } },
    ],
  ])('refuses %s', (_name, raw) => {
    expect(parseEntry(raw)).toBeNull();
  });

  it('accepts an https image and a picture carried in blob parts', () => {
    const base = { k: 'image', pg: 'home', at: [0, 0], size: [5, 5] };
    expect(
      parseEntry({ ...good, p: { ...base, src: 'https://files.example/a.png?x=1' } }),
    ).not.toBeNull();
    expect(
      parseEntry({ ...good, p: { ...base, src: '', mime: 'image/png', parts: 3 } }),
    ).not.toBeNull();
  });

  it.each([
    ['an inline data URL', { src: 'data:image/png;base64,iVBORw0KGgo=' }],
    ['svg blob', { src: '', mime: 'image/svg+xml', parts: 1 }],
    ['no parts', { src: '', mime: 'image/png' }],
    ['zero parts', { src: '', mime: 'image/png', parts: 0 }],
    ['too many parts', { src: '', mime: 'image/png', parts: 21 }],
    ['an https address with a quote', { src: 'https://x/y"onload=1' }],
  ])('refuses an image with %s', (_n, extra) => {
    const p = { k: 'image', pg: 'home', at: [0, 0], size: [5, 5], ...extra };
    expect(parseEntry({ ...good, p })).toBeNull();
  });
});
